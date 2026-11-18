use anyhow::Result;
use std::path::PathBuf;

/// Ask the OS for a folder. Linux reuses the GTK toolkit the window is built
/// on; macOS and Windows shell out to their own dialogs, so no new dependency
/// is needed for a feature the platform already ships.
pub fn folder() -> Result<Option<PathBuf>> {
    #[cfg(target_os = "linux")]
    {
        use gtk::prelude::*;
        let dialog = gtk::FileChooserDialog::with_buttons(
            Some("Add project folder"),
            None::<&gtk::Window>,
            gtk::FileChooserAction::SelectFolder,
            &[
                ("Cancel", gtk::ResponseType::Cancel),
                ("Add Project", gtk::ResponseType::Accept),
            ],
        );
        dialog.set_modal(true);
        dialog.set_position(gtk::WindowPosition::Center);
        let response = dialog.run();
        let picked = if response == gtk::ResponseType::Accept {
            dialog.filename()
        } else {
            None
        };
        /* close() releases the modal grab; destroying it while it is still
           grabbed leaves the main window deaf to pointer events. */
        dialog.close();
        Ok(picked)
    }
    #[cfg(target_os = "macos")]
    {
        let out = std::process::Command::new("osascript")
            .args([
                "-e",
                "POSIX path of (choose folder with prompt \"Add project folder\")",
            ])
            .output()?;
        if !out.status.success() {
            return Ok(None);
        }
        let path = String::from_utf8(out.stdout)?.trim().to_string();
        Ok(if path.is_empty() { None } else { Some(PathBuf::from(path)) })
    }
    #[cfg(target_os = "windows")]
    {
        let script = concat!(
            "Add-Type -AssemblyName System.Windows.Forms;",
            "$d=New-Object System.Windows.Forms.FolderBrowserDialog;",
            "$d.Description='Add project folder';",
            "if($d.ShowDialog() -eq 'OK'){$d.SelectedPath}"
        );
        let out = std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", script])
            .output()?;
        if !out.status.success() {
            return Ok(None);
        }
        let path = String::from_utf8(out.stdout)?.trim().to_string();
        Ok(if path.is_empty() { None } else { Some(PathBuf::from(path)) })
    }
}
