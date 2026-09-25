import { chromium } from "playwright";
import { mkdir } from "node:fs/promises";

const base = process.env.KEPLR_URL || "http://127.0.0.1:7137";
const output = process.env.SHOWCASE_OUTPUT || "showcase-output";
const open = "crates%2Fkeplr-ui%2Fsrc%2Fworkbench.rs";

await mkdir(output, { recursive: true });
const browser = await chromium.launch({ headless: true });
const page = await browser.newPage({
  viewport: { width: 2560, height: 1264 },
  deviceScaleFactor: 1,
});

async function openPage(query) {
  await page.goto(`${base}/${query}`, { waitUntil: "domcontentloaded" });
  await page.waitForSelector("#pane-surface", { timeout: 30000 });
  await page.waitForSelector(".pane-card", { timeout: 30000 });
  await page.waitForTimeout(1800);
}

async function capture(name) {
  await page.screenshot({ path: `${output}/${name}.png` });
}

await openPage(`?open=${open}`);
await capture("shot-editor");

await page.locator("#layout-menu").click();
await page.waitForTimeout(300);
await capture("shot-layout");
await page.keyboard.press("Escape");

await openPage(`?open=${open}&bottom=terminal`);
await capture("shot-terminal");

await openPage(`?open=${open}&left=source`);
await capture("shot-git");

await openPage(`?open=${open}&bottom=serial`);
await capture("shot-serial");

await openPage("");
await page.locator("#finder").click();
await page.keyboard.type("workbench");
await page.waitForTimeout(400);
await capture("shot-spotlight");

await browser.close();
