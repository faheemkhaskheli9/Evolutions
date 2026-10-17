// End-to-end smoke test: build output served by `vite preview`, driven in Chromium.
// Usage: npm run build && npm run test:e2e   (set SHOTS=dir to save screenshots)
import { spawn } from "node:child_process";
import { existsSync, mkdirSync } from "node:fs";
import { chromium } from "playwright-core";

const PORT = 4179;
const shots = process.env.SHOTS;
if (shots) mkdirSync(shots, { recursive: true });

// Prefer the headless shell from a pre-installed Playwright browser cache; otherwise let
// Playwright use its default browser.
const executablePath = [
  process.env.CHROMIUM_PATH,
  "/opt/pw-browsers/chromium_headless_shell-1194/chrome-linux/headless_shell",
].find((p) => p && existsSync(p));
const server = spawn(process.execPath, ["node_modules/vite/bin/vite.js", "preview", "--port", String(PORT), "--strictPort"], { stdio: "pipe" });
process.on("exit", () => server.kill());
await new Promise((resolve, reject) => {
  server.stdout.on("data", (d) => String(d).includes(String(PORT)) && resolve());
  server.on("exit", (c) => reject(new Error(`preview exited ${c}`)));
  setTimeout(() => reject(new Error("preview did not start")), 20000);
});

const browser = await chromium.launch({ executablePath, args: ["--no-sandbox"] });
const page = await browser.newPage({ viewport: { width: 1400, height: 860 } });
const errors = [];
page.on("pageerror", (e) => errors.push(e.message));
page.on("console", (m) => m.type() === "error" && errors.push(m.text()));

const step = async (name, fn) => {
  process.stdout.write(`• ${name} … `);
  await fn();
  if (shots) await page.screenshot({ path: `${shots}/${name.replace(/\W+/g, "-")}.png` });
  console.log("ok");
};

try {
  await page.goto(`http://localhost:${PORT}/`);
  await step("engine loads and shows the new-world dialog", async () => {
    await page.getByText("Create a world").waitFor({ timeout: 20000 });
  });
  await step("customise world settings", async () => {
    await page.getByRole("button", { name: /Earth-like Continent/ }).click();
    await page.getByLabel("Customise world settings").check();
    await page.getByLabel("World size").selectOption({ label: "Small (96 × 48)" });
    await page.getByLabel("World size").selectOption({ label: "Medium (128 × 64)" });
  });
  await step("create Earth-like world", async () => {
    await page.getByRole("button", { name: "Create world" }).click();
    await page.locator(".badge").waitFor({ timeout: 30000 });
  });
  await step("play in Individual mode", async () => {
    await page.getByRole("radio", { name: "1 week/s" }).click();
    await page.getByRole("button", { name: "Play" }).click();
    await page.waitForTimeout(4000);
    const date = await page.locator(".date").textContent();
    if (!/Year 0 · (Jan|Feb|Mar)/.test(date ?? "")) throw new Error(`unexpected date ${date}`);
  });
  await step("inspect an animal", async () => {
    await page.getByRole("button", { name: "Pause" }).click();
    await page.waitForTimeout(300);
    const pos = await page.evaluate(() => {
      const m = window.__evoMap;
      const a = m.firstAnimal();
      return a ? m.toScreen(a[0], a[1]) : null;
    });
    if (!pos) throw new Error("no animals in frame");
    await page.mouse.click(pos[0], pos[1]);
    await page.getByText("Energy reserves").waitFor({ timeout: 5000 });
  });
  await step("jump 1000 years in Population mode", async () => {
    await page.getByRole("button", { name: "+1k yr" }).click();
    await page.getByText("Fast-forwarding…").waitFor();
    await page.getByText("Fast-forwarding…").waitFor({ state: "detached", timeout: 180000 });
    const badge = await page.locator(".badge").textContent();
    if (!badge.includes("Population")) throw new Error(`mode ${badge}`);
  });
  await step("species panel shows history", async () => {
    await page.getByRole("tab", { name: "Species" }).click();
    await page.getByText("Population", { exact: true }).first().waitFor();
  });
  await step("tree and chronicle render", async () => {
    await page.getByRole("tab", { name: "Tree" }).click();
    await page.locator("svg.phylo").waitFor();
    await page.getByRole("tab", { name: /Chronicle/ }).click();
    await page.locator(".events li").first().waitFor();
  });
  await step("population layer for one species", async () => {
    await page.getByRole("tab", { name: "World" }).click();
    await page.getByRole("button", { name: "Population density" }).click();
    const select = page.getByLabel("Density layer species");
    const id = await select.locator("option").nth(1).getAttribute("value");
    await select.selectOption(id);
    await page.waitForTimeout(500);
  });
  await step("run until a threshold", async () => {
    await page.getByRole("button", { name: "Run until…" }).click();
    await page.getByLabel("Stop condition").selectOption("population");
    await page.getByLabel("Direction").selectOption("above");
    await page.getByLabel("Threshold").fill("1");
    await page.getByLabel("Maximum years").selectOption("100");
    await page.getByRole("button", { name: "Run", exact: true }).click();
    // Already above 1, so the run goes the full length and says so.
    await page.getByText(/condition already holds/).waitFor({ timeout: 120000 });
  });
  await step("watch region", async () => {
    await page.getByRole("button", { name: "Terrain & biomes" }).click();
    await page.getByRole("button", { name: /Watch region/ }).click();
    const box = await page.locator("canvas.map").boundingBox();
    await page.mouse.move(box.x + box.width * 0.3, box.y + box.height * 0.3);
    await page.mouse.down();
    await page.mouse.move(box.x + box.width * 0.42, box.y + box.height * 0.45, { steps: 8 });
    await page.mouse.up();
    await page.waitForTimeout(1500);
  });
  await step("climate intervention is logged", async () => {
    await page.getByRole("button", { name: "-2 °C" }).click();
    await page.getByRole("tab", { name: /Chronicle/ }).click();
    await page.getByText(/Climate shift/).first().waitFor();
  });
  await step("autosave offers to resume after a reload", async () => {
    await page.waitForTimeout(500);
    await page.reload();
    await page.getByText("Continue where you left off?").waitFor({ timeout: 20000 });
    await page.getByRole("button", { name: "Resume" }).click();
    await page.locator(".badge").waitFor({ timeout: 30000 });
  });
  const fatal = errors.filter((e) => !/favicon/.test(e));
  if (fatal.length) throw new Error(`page errors:\n${fatal.join("\n")}`);
  console.log("\nSmoke test passed.");
} catch (e) {
  console.error("\nFAILED:", e.message);
  if (shots) await page.screenshot({ path: `${shots}/failure.png` });
  process.exitCode = 1;
} finally {
  await browser.close();
  server.kill();
}
