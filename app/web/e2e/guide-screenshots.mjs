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
  await page.locator("table.founders").scrollIntoViewIfNeeded();
  await shot("03-founders", page.locator("table.founders"));
  await page.getByLabel("Customise world settings").check();
  await page.locator(".settings").evaluate((s) => s.scrollIntoView({ block: "center" }));
  await shot("02-world-settings", page.locator(".modal"));
  await page.getByLabel("Customise world settings").uncheck();
  await page.getByRole("button", { name: "Save as template…" }).click();
  await page.getByLabel("Template name").fill("Lesson 1: cold north");
  await page.getByLabel("Template description").fill("Hares, deer and wolves. Compare the fur of northern and southern hares after 2,000 years.");
  await page.getByRole("button", { name: "Save template" }).click();
  await page.getByRole("button", { name: /^Lesson 1/ }).waitFor();
  await noToasts();
  await page.getByRole("button", { name: "Save as template…" }).click();
  await page.getByLabel("Template name").fill("Lesson 2: island deer");
  await page.locator(".user-templates").scrollIntoViewIfNeeded();
  await shot("27-templates", page.locator(".user-templates"));
  await page.getByRole("button", { name: "Cancel" }).click();
  await page.getByRole("button", { name: "Create world" }).click();
  await page.locator(".badge").waitFor({ timeout: 30000 });
  await page.waitForTimeout(800);
  await shot("04-new-world");

  console.log("2. Watching individuals");
  await page.getByRole("radio", { name: "1 day/s" }).click();
  await page.getByRole("button", { name: "Play", exact: true }).click();
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
  await page.getByRole("tab", { name: "World" }).click();
  await page.getByRole("radio", { name: "Macro", exact: true }).click();
  await jump("+10k yr");
  await page.waitForTimeout(800);
  await shot("26-macro-mode");
  await page.getByRole("radio", { name: "Auto", exact: true }).click();
  await page.waitForTimeout(500);

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
  const adaptation = page.locator(".ev-adaptation", { has: page.locator(".ev-why") }).first();
  await adaptation.locator(".ev-why").click();
  await adaptation.locator(".why p").waitFor();
  await adaptation.locator(".why summary").click();
  await page.waitForTimeout(300);
  await shot("24-why", adaptation);
  await page.getByRole("tab", { name: "Species" }).click();
  await page.getByLabel("Trait to chart").selectOption({ label: "Boldness" });
  await page.getByRole("button", { name: /^Why did .* change\?/ }).click();
  await page.locator(".trait-why .why-verdict").waitFor();
  await page.locator(".trait-why").scrollIntoViewIfNeeded();
  await shot("25-trait-why", page.locator(".trait-why"));

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
  const b = await mapBox();
  const [rx, ry] = [b.x + b.width * 0.62, b.y + b.height * 0.55];
  // A box holding more animals than the Individual-mode cap is refused; try smaller ones.
  let size = 0;
  for (size of [0.12, 0.08, 0.05, 0.03]) {
    await page.getByRole("button", { name: /Watch region/ }).click();
    await page.mouse.move(rx, ry);
    await page.mouse.down();
    await page.mouse.move(rx + b.width * size, ry + b.height * size * 1.5, { steps: 10 });
    await page.mouse.up();
    await page.waitForTimeout(1500);
    if (/region/.test((await page.locator(".badge").textContent()) ?? "")) break;
  }
  await noToasts();
  await zoomAt(rx + (b.width * size) / 2, ry + (b.height * size * 1.5) / 2, 4);
  await page.getByRole("radio", { name: "1 hour/s" }).click();
  await page.getByRole("button", { name: "Play", exact: true }).click();
  await page.waitForTimeout(2500);
  await page.getByRole("button", { name: "Pause" }).click();
  await noToasts();
  await shot("19-watch-region");
  await zoomAt(rx + (b.width * size) / 2, ry + (b.height * size * 1.5) / 2, -4);

  console.log("8. Interventions");
  await page.getByRole("radio", { name: "Population" }).click();
  await page.waitForTimeout(800);
  await page.getByRole("heading", { name: "Change the world" }).evaluate((el) => el.scrollIntoView({ block: "start" }));
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

  console.log("9. Edit the world");
  await page.getByRole("tab", { name: "World" }).click();
  await page.getByRole("button", { name: "✎ Edit world" }).click();
  await page.getByRole("radio", { name: "Flat map" }).click();
  await page.waitForTimeout(500);
  const e = await mapBox();
  // A strait across the continent, then a mountain ridge beside it.
  for (const [brush, size, [fx0, fy0, fx1, fy1]] of [
    ["Sea", "1.5", [0.62, 0.3, 0.62, 0.85]],
    ["▲ Raise", "3", [0.68, 0.45, 0.8, 0.7]],
    ["▲ Raise", "3", [0.69, 0.45, 0.81, 0.7]],
  ]) {
    await page.getByRole("radio", { name: brush }).click();
    await page.getByLabel("Brush size").fill(size);
    await page.getByLabel("Brush strength").fill("1");
    await page.mouse.move(e.x + e.width * fx0, e.y + e.height * fy0);
    await page.mouse.down();
    await page.mouse.move(e.x + e.width * fx1, e.y + e.height * fy1, { steps: 14 });
    await page.mouse.up();
    await page.waitForTimeout(800);
  }
  await page.getByRole("heading", { name: "Edit world" }).evaluate((el) => el.scrollIntoView({ block: "start" }));
  await page.mouse.move(e.x + e.width * 0.6, e.y + e.height * 0.45);
  await noToasts();
  await shot("28-edit-world");
  await page.getByRole("button", { name: "✓ Done editing" }).click();
  await page.getByRole("radio", { name: "3D world" }).click();

  console.log("10. Design a species");
  await page.getByRole("tab", { name: "World" }).click();
  await page.getByRole("button", { name: "Design…" }).click();
  await page.getByLabel("Species name").fill("Snow grazer");
  await page.getByLabel("Species colour").fill("#e8e2d0");
  await page.getByLabel("Description").fill("A heavier, thick-coated hare for the cold north.");
  await page.getByLabel("Adult body mass", { exact: true }).fill("480");
  await page.getByLabel("Insulation (fur/fat)", { exact: true }).fill("900");
  await page.getByLabel("Appendage size (ears, tail)", { exact: true }).fill("300");
  await page.getByRole("cell", { name: "This world" }).waitFor();
  await page.waitForTimeout(1200);
  await shot("29-species-designer", page.locator(".modal.designer"));
  await page.getByRole("button", { name: "Cancel" }).click();

  console.log("11. Branches: what if the climate cooled?");
  await page.getByRole("button", { name: "⑂ Branch" }).click();
  await page.getByText(/^Started "Branch 2"/).waitFor({ timeout: 20000 });
  page.once("dialog", (d) => d.accept("Colder"));
  await page.getByRole("button", { name: "Rename branch" }).click();
  await page.getByRole("tab", { name: "World" }).click();
  await page.getByRole("button", { name: "-2 °C" }).click();
  await page.getByRole("button", { name: "-2 °C" }).click();
  await jump("+1k yr");
  const branchSelect = page.getByRole("combobox", { name: "Branch", exact: true });
  await branchSelect.selectOption(await branchSelect.locator("option", { hasText: /^Main/ }).getAttribute("value"));
  await page.getByText(/^Switched to "Main"/).waitFor({ timeout: 20000 });
  await jump("+1k yr");
  await noToasts();
  await page.getByRole("button", { name: /^Checkpoint, year .* \(before a jump\)$/ }).last().dispatchEvent("click");
  await page.getByRole("menuitem", { name: "Rewind here" }).waitFor();
  await shot("30-branches", page.locator(".timebar"));
  await page.getByRole("button", { name: /^Checkpoint, year .* \(before a jump\)$/ }).last().dispatchEvent("click");
  await page.getByRole("button", { name: "Compare…" }).click();
  const cmp = page.getByRole("dialog", { name: "Compare branches" });
  await cmp.locator(".compare-grid svg").first().waitFor({ timeout: 20000 });
  await cmp.getByRole("combobox", { name: "Trait to compare" }).selectOption({ label: "Insulation (fur/fat)" });
  await page.waitForTimeout(500);
  await shot("31-compare-branches", page.locator(".modal.compare-dialog"));
  await cmp.getByRole("button", { name: "Close" }).click();

  console.log("12. Resume after reload");
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
