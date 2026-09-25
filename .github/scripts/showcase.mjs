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
const browserErrors = [];
page.on("console", message => {
  const line = `[browser:${message.type()}] ${message.text()}`;
  browserErrors.push(line);
  console.log(line);
});
page.on("pageerror", error => {
  const line = `[pageerror] ${error.stack || error}`;
  browserErrors.push(line);
  console.log(line);
});
page.on("requestfailed", request => {
  const line = `[requestfailed] ${request.url()} ${request.failure()?.errorText || "unknown"}`;
  browserErrors.push(line);
  console.log(line);
});

async function openPage(query) {
  const response = await page.goto(`${base}/${query}`, { waitUntil: "domcontentloaded" });
  console.log(`[page] ${response?.status() || "no-status"} ${page.url()}`);
  try {
    await page.waitForSelector("#pane-surface", { state: "attached", timeout: 30000 });
    const paneState = await page.locator("#pane-surface").evaluate(element => {
      const rect = element.getBoundingClientRect();
      const style = getComputedStyle(element);
      return {
        rect: { x: rect.x, y: rect.y, width: rect.width, height: rect.height },
        display: style.display,
        visibility: style.visibility,
        hidden: element.hidden,
        parent: element.parentElement?.id || null,
      };
    });
    console.log(`[pane] ${JSON.stringify(paneState)}`);
    const domState = await page.evaluate(() => {
      const ids = ["workbench", "pane-surface", "left", "center", "editorwrap", "fallback", "bottom"];
      return Object.fromEntries(ids.map(id => {
        const element = document.getElementById(id);
        return [id, element ? {
          connected: element.isConnected,
          parent: element.parentElement?.id || null,
          parentClass: element.parentElement?.className || null,
        } : null];
      }));
    });
    console.log(`[dom] ${JSON.stringify(domState)}`);
    await page.waitForSelector(".pane-card", { state: "visible", timeout: 30000 });
    await page.waitForFunction(() => {
      const wrap = document.getElementById("editorwrap");
      const fallback = document.getElementById("fallback");
      return !!wrap && (!!wrap.querySelector(".cm-editor") || (!!fallback && getComputedStyle(fallback).display !== "none"));
    }, { timeout: 30000 });
    await page.waitForTimeout(1800);
  } catch (error) {
    const state = await page.evaluate(() => ({
      readyState: document.readyState,
      paneCount: document.querySelectorAll("#pane-surface").length,
      bodyText: document.body?.innerText?.slice(0, 800) || "",
      html: document.documentElement?.outerHTML?.slice(0, 4000) || "",
    })).catch(() => null);
    console.log(`[debug] ${JSON.stringify(state)}`);
    console.log(`[errors] ${JSON.stringify(browserErrors)}`);
    throw error;
  }
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
