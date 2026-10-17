// Captures the screenshots used in docs/USER_GUIDE.md.
// Usage: npm run build && node e2e/guide-screenshots.mjs [outDir]   (default ../../docs/guide)
import { spawn } from "node:child_process";
import { existsSync, mkdirSync } from "node:fs";
import { chromium } from "playwright-core";

const PORT = 4181;
const out = process.argv[2] ?? "../../docs/guide";
mkdirSync(out, { recursive: true });

const executablePath = [process.env.CHROMIUM_PATH, "/opt/pw-browsers/chromium_headless_shell-1194/chrome-linux/headless_shell"].find(
  (p) => p && existsSync(p),
);
const server = spawn(process.execPath, ["node_modules/vite/bin/vite.js", "preview", "--port", String(PORT), "--strictPort"], { stdio: "pipe" });
process.on("exit", () => server.kill());
await new Promise((resolve, reject) => {
  server.stdout.on("data", (d) => String(d).includes(String(PORT)) && resolve());
  server.on("exit", (c) => reject(new Error(`preview exited ${c}`)));
  setTimeout(() => reject(new Error("preview did not start")), 20000);
});

const browser = await chromium.launch({ executablePath, args: ["--no-sandbox"] });
const page = await browser.newPage({ viewport: { width: 1440, height: 900 }, colorScheme: "light", deviceScaleFactor: 1 });

const shot = async (name, target) => {
  const path = `${out}/${name}.png`;
  if (target) await target.screenshot({ path });
  else await page.screenshot({ path });
  console.log(`  saved ${path}`);
};
const mapBox = () => page.locator("canvas.map").boundingBox();
const animalOnScreen = () =>
  page.evaluate(() => {
    const m = window.__evoMap;
    const a = m.firstAnimal();
    return a ? m.toScreen(a[0], a[1]) : null;
  });
// Positive clicks zoom in, negative zoom out (same point → exact undo).
const zoomAt = async (x, y, clicks) => {
  await page.mouse.move(x, y);
  for (let i = 0; i < Math.abs(clicks); i++) {
    await page.mouse.wheel(0, clicks > 0 ? -240 : 240);
    await page.waitForTimeout(60);
  }
};
const noToasts = () => page.locator(".toast").first().waitFor({ state: "detached", timeout: 15000 }).catch(() => {});
const jump = async (label) => {
  await page.getByRole("button", { name: label }).click();
  await page.getByText("Fast-forwarding…").waitFor();
  await page.getByText("Fast-forwarding…").waitFor({ state: "detached", timeout: 900000 });
};

try {
  await page.goto(`http://localhost:${PORT}/`);
  await page.getByText("Create a world").waitFor({ timeout: 20000 });

  console.log("1. Creating a world");
  await page.getByRole("button", { name: /Earth-like Continent/ }).click();
  await page.getByLabel("Seed").fill("2024");
  await shot("01-create-world", page.locator(".modal"));
  await page.getByLabel("Customise world settings").check();
  await page.locator(".modal").evaluate((m) => (m.scrollTop = 260));
  await shot("02-world-settings", page.locator(".modal"));
  await page.getByLabel("Customise world settings").uncheck();
  await page.getByRole("button", { name: "Create world" }).click();
  await page.locator(".badge").waitFor({ timeout: 30000 });
  await page.waitForTimeout(800);
  await shot("04-new-world");

  console.log("2. Watching individuals");
  await page.getByRole("radio", { name: "1 day/s" }).click();
  await page.getByRole("button", { name: "Play" }).click();
  await page.waitForTimeout(5000);
  await page.getByRole("button", { name: "Pause" }).click();
  const zoomPoint = await animalOnScreen();
  await zoomAt(zoomPoint[0], zoomPoint[1], 5);
  await page.waitForTimeout(400);
  await shot("05-individuals-zoomed");
  const pos = await animalOnScreen();
  await page.mouse.click(pos[0], pos[1]);
  await page.getByText("Energy reserves").waitFor({ timeout: 5000 });
  await page.waitForTimeout(600);
  await shot("06-inspector");
  await shot("07-time-controls", page.locator(".timebar"));
  await zoomAt(zoomPoint[0], zoomPoint[1], -5);
  await page.waitForTimeout(300);

  console.log("3. Fast-forwarding");
  await page.getByRole("button", { name: "+1k yr" }).click();
  await page.getByText("Fast-forwarding…").waitFor();
  await page.waitForTimeout(2500);
  await shot("08-jump-progress");
  await page.getByText("Fast-forwarding…").waitFor({ state: "detached", timeout: 900000 });
  await jump("+1k yr");
  await page.waitForTimeout(800);
  await shot("09-population-mode");

  console.log("4. Species, tree, chronicle");
  await page.getByRole("tab", { name: "Species" }).click();
  await page.getByText("Population", { exact: true }).first().waitFor();
  await page.waitForTimeout(1800);
  await shot("10-species", page.locator(".side"));
  await page.getByLabel("Trait to chart").selectOption({ label: "Litter size" });
  await page.locator(".tab-content").evaluate((el) => (el.scrollTop = 420));
  await page.waitForTimeout(400);
  await shot("11-trait-history", page.locator(".side"));
  await page.getByRole("tab", { name: "Tree" }).click();
  await page.locator("svg.phylo").waitFor();
  await shot("12-tree", page.locator(".side"));
  await page.getByRole("tab", { name: /Chronicle/ }).click();
  await page.locator(".events li").first().waitFor();
  await page.locator(".tab-content").evaluate((el) => (el.scrollTop = 0));
  await shot("13-chronicle", page.locator(".side"));

  console.log("5. Map layers");
  await page.getByRole("tab", { name: "World" }).click();
  for (const [label, name] of [
    ["Temperature (today)", "14-layer-temperature"],
    ["Rainfall", "15-layer-rainfall"],
    ["Vegetation", "16-layer-vegetation"],
    ["Population density", "17-layer-density"],
  ]) {
    await page.getByRole("button", { name: label }).click();
    await page.waitForTimeout(1500);
    await shot(name);
  }
  await page.getByRole("button", { name: "Terrain & biomes" }).click();

  console.log("6. Run until");
  await page.getByRole("button", { name: "Run until…" }).click();
  await page.getByLabel("Stop condition").selectOption("trait");
  await page.waitForTimeout(300);
  await shot("18-run-until", page.locator(".popover"));
  await page.getByRole("button", { name: "Run until…" }).click();

  console.log("7. Watch a region");
  await page.getByRole("button", { name: /Watch region/ }).click();
  const b = await mapBox();
  const [rx, ry] = [b.x + b.width * 0.62, b.y + b.height * 0.55];
  await page.mouse.move(rx, ry);
  await page.mouse.down();
  await page.mouse.move(rx + b.width * 0.12, ry + b.height * 0.18, { steps: 10 });
  await page.mouse.up();
  await page.waitForTimeout(1500);
  await page.getByRole("radio", { name: "1 hour/s" }).click();
  await page.getByRole("button", { name: "Play" }).click();
  await page.waitForTimeout(2500);
  await page.getByRole("button", { name: "Pause" }).click();
  await noToasts();
  await shot("19-watch-region");

  console.log("8. Interventions");
  await page.getByRole("radio", { name: "Population" }).click();
  await page.waitForTimeout(800);
  await page.locator(".tab-content").evaluate((el) => (el.scrollTop = 380));
  await noToasts();
  await shot("20-interventions", page.locator(".side"));
  await page.getByRole("button", { name: "Population density" }).click();
  await page.getByRole("button", { name: "Arm meteor" }).click();
  const m = await mapBox();
  await page.mouse.click(m.x + m.width * 0.74, m.y + m.height * 0.58);
  await page.waitForTimeout(2500);
  await noToasts();
  await shot("21-meteor");
  await page.getByRole("button", { name: "Terrain & biomes" }).click();
  await page.getByRole("tab", { name: /Chronicle/ }).click();
  await page.getByText(/Meteor impact/).first().waitFor();
  await page.locator(".tab-content").evaluate((el) => (el.scrollTop = 0));
  await shot("22-meteor-chronicle", page.locator(".side"));

  console.log("9. Resume after reload");
  await page.waitForTimeout(600);
  await page.reload();
  await page.getByText("Continue where you left off?").waitFor({ timeout: 20000 });
  await shot("23-resume", page.locator(".modal"));
  console.log("Done.");
} catch (e) {
  console.error("FAILED:", e.message);
  await page.screenshot({ path: `${out}/failure.png` });
  process.exitCode = 1;
} finally {
  await browser.close();
  server.kill();
}
