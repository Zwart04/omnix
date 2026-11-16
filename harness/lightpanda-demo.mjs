// Demo: kendalikan Lightpanda lewat CDP dengan Puppeteer.
// Jalankan dulu: omnix browser serve --port 9222
// Lalu: npm init -y && npm install puppeteer-core && node lightpanda-demo.mjs https://example.com

import puppeteer from "puppeteer-core";

const url = process.argv[2] ?? "https://example.com";
const endpoint = process.env.OMNIX_CDP ?? "ws://127.0.0.1:9222";

const browser = await puppeteer.connect({ browserWSEndpoint: endpoint });
try {
  const page = await browser.newPage();
  await page.goto(url, { waitUntil: "networkidle", timeout: 30_000 });

  console.log("title:", await page.title());
  console.log("url:", page.url());

  const text = await page.locator("body").innerText();
  console.log("\nread:\n" + text.slice(0, 4000));

  const tree = await page.accessibility.snapshot({ interestingOnly: true });
  console.log("\nsnapshot:\n" + JSON.stringify(tree, null, 2).slice(0, 8000));

  await page.close();
} finally {
  await browser.disconnect();
}
