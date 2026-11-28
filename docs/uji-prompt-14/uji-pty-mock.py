#!/usr/bin/env python3
"""Uji otomatis protokol PTY tanpa kuota agen sungguhan.

Memverifikasi 3 skenario inti Prompt 14 pada mock TUI bergaya Codex:
1. Trust modal + single-write blind timer -> mereproduksi kehilangan prompt
   (bukti mekanisme bug).
2. Kirim setelah OSC 7501 idle + ?2004h dengan tempelan & CR terpisah (jeda
   250 ms) -> brief multibaris diterima utuh dan status berubah ke working/done.
3. Kirim ke proses yang tidak menerima input -> terdeteksi not-accepted.
"""

import fcntl
import os
import pty
import re
import select
import signal
import struct
import subprocess
import sys
import termios
import time

MOCK_TUI_CODE = r"""
import os, select, sys, termios, tty

mode = sys.argv[1]
fd = sys.stdin.fileno()
old = termios.tcgetattr(fd)
tty.setraw(fd)
try:
    # Emulate Codex startup: enable ?2004h early, enter alt-screen, draw loading
    os.write(1, b"\x1b[?2004h\x1b[?1049h\x1b[?25l>_ MockCodex loading\r\n")
    time_mod = __import__("time")
    if mode == "trust-modal":
        time_mod.sleep(0.15)
        os.write(1, b"Folder access\r\nTrust this folder?\r\n> 1. Trust and continue\r\n")
        # Wait for input; discard bracketed paste while in trust modal, accept \r to close modal
        buf = b""
        t_end = time_mod.monotonic() + 2.0
        while time_mod.monotonic() < t_end:
            r, _, _ = select.select([fd], [], [], 0.05)
            if r:
                buf += os.read(fd, 8192)
                if b"\r" in buf:
                    os.write(1, b"\x1b[2J>_ MockCodex\r\n> Ask Codex to do anything\r\n\x1b]7501;state=idle:app=codex\x1b\\")
                    break
        time_mod.sleep(0.4)
    elif mode == "ready-split":
        time_mod.sleep(0.15)
        os.write(1, b"\x1b[?25h>_ MockCodex\r\n> Ask Codex to do anything\r\n\x1b]7501;state=idle:app=codex\x1b\\")
        pasted = None
        buf = b""
        t_end = time_mod.monotonic() + 3.0
        while time_mod.monotonic() < t_end:
            r, _, _ = select.select([fd], [], [], 0.05)
            if r:
                chunk = os.read(fd, 8192)
                buf += chunk
                if pasted is None and b"\x1b[200~" in buf and b"\x1b[201~" in buf:
                    s = buf.index(b"\x1b[200~") + 6
                    e = buf.index(b"\x1b[201~")
                    pasted = buf[s:e]
                    tail = buf[e + 6:]
                    # Strict TUI: requires \r in a SEPARATE read after paste settled
                    if b"\r" in tail:
                        os.write(1, b"[HELD-SAME-WRITE]")
                    else:
                        os.write(1, b"[PASTE-IN-COMPOSER]")
                    buf = b""
                elif pasted is not None and b"\r" in buf:
                    os.write(1, b"\x1b]7501;state=working:app=codex\x1b\\")
                    os.write(1, b"SUBMITTED_LEN=" + str(len(pasted)).encode() + b"\r\n")
                    time_mod.sleep(0.08)
                    os.write(1, b"\x1b]7501;state=done:app=codex\x1b\\")
                    break
        time_mod.sleep(0.1)
    elif mode == "stuck":
        os.write(1, b"\x1b]7501;state=idle:app=codex\x1b\\")
        time_mod.sleep(1.2)
finally:
    termios.tcsetattr(fd, termios.TCSADRAIN, old)
"""


def run_case(mode: str, action) -> bytes:
    master, slave = pty.openpty()
    fcntl.ioctl(master, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 80, 0, 0))
    proc = subprocess.Popen(
        [sys.executable, "-c", MOCK_TUI_CODE, mode],
        stdin=slave,
        stdout=slave,
        stderr=slave,
        preexec_fn=os.setsid,
        close_fds=True,
    )
    os.close(slave)
    fl = fcntl.fcntl(master, fcntl.F_GETFL)
    fcntl.fcntl(master, fcntl.F_SETFL, fl | os.O_NONBLOCK)
    rx = bytearray()
    try:
        action(master, rx)
        t_end = time.monotonic() + 1.0
        while time.monotonic() < t_end and proc.poll() is None:
            r, _, _ = select.select([master], [], [], 0.05)
            if r:
                try:
                    c = os.read(master, 8192)
                    if c:
                        rx.extend(c)
                except OSError:
                    break
    finally:
        if proc.poll() is None:
            try:
                os.killpg(os.getpgid(proc.pid), signal.SIGTERM)
                proc.wait(timeout=1)
            except Exception:
                pass
        os.close(master)
    return bytes(rx)


def wait_for(master: int, rx: bytearray, needle: bytes, timeout: float = 2.0) -> bool:
    t_end = time.monotonic() + timeout
    while time.monotonic() < t_end:
        if needle in rx:
            return True
        r, _, _ = select.select([master], [], [], 0.03)
        if r:
            try:
                c = os.read(master, 8192)
                if c:
                    rx.extend(c)
            except OSError:
                break
    return needle in rx


def main() -> int:
    # 1. Reproduce trust-modal swallowing blind paste + \r
    def act_trust(master, rx):
        assert wait_for(master, rx, b"Trust this folder?")
        os.write(master, b"\x1b[200~brief baris 1\nbaris 2\x1b[201~\r")
        wait_for(master, rx, b"state=idle:app=codex", 1.5)

    out1 = run_case("trust-modal", act_trust)
    assert b"\x1b[?2004h" in out1, "2004h must appear early"
    assert b"Ask Codex to do anything" in out1, "modal dismissed to empty composer"
    assert b"state=working" not in out1, "brief must NOT have started a turn"
    print("PASS mock-pty-1: trust modal menelan tempelan + \\r ke komposer kosong (repro akurat)")

    # 2. Split write after ?2004h + OSC 7501 idle
    prompt = b"Baris 1 `src/gui.rs`\nBaris 2: balas OK"

    def act_split(master, rx):
        assert wait_for(master, rx, b"\x1b[?2004h")
        assert wait_for(master, rx, b"state=idle:app=codex")
        os.write(master, b"\x1b[200~" + prompt + b"\x1b[201~")
        assert wait_for(master, rx, b"[PASTE-IN-COMPOSER]", 1.0)
        time.sleep(0.25)
        os.write(master, b"\r")
        assert wait_for(master, rx, b"state=working:app=codex", 1.0)
        assert wait_for(master, rx, b"state=done:app=codex", 1.0)

    out2 = run_case("ready-split", act_split)
    assert f"SUBMITTED_LEN={len(prompt)}".encode() in out2
    print("PASS mock-pty-2: tunggu ?2004h+idle + tempel terpisah + jeda 250ms + \\r -> working -> done")

    # 3. Stuck TUI -> no working transition -> detectable as not-accepted
    def act_stuck(master, rx):
        assert wait_for(master, rx, b"state=idle:app=codex")
        os.write(master, b"\x1b[200~halo\x1b[201~")
        time.sleep(0.2)
        os.write(master, b"\r")
        found_work = wait_for(master, rx, b"state=working:app=codex", 0.6)
        assert not found_work, "stuck process must not report working"

    out3 = run_case("stuck", act_stuck)
    assert b"state=working" not in out3
    print("PASS mock-pty-3: TUI macet tidak memicu working -> dapat dideteksi sebagai not-accepted")
    return 0


if __name__ == "__main__":
    sys.exit(main())
