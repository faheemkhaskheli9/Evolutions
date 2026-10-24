// End-to-end smoke test: build output served by `vite preview`, driven in Chromium.
// Usage: npm run build && npm run test:e2e   (set SHOTS=dir to save screenshots)
import { spawn } from "node:child_process";
import { existsSync, mkdirSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
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
    const view = await page.locator("canvas.map").boundingBox();
    if (pos[0] < view.x || pos[0] > view.x + view.width || pos[1] < view.y || pos[1] > view.y + view.height)
      throw new Error(`first animal is off screen at ${pos.map(Math.round)}`);
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
  await step("allele frequency layer", async () => {
    await page.getByRole("button", { name: "Allele frequency" }).click();
    const allele = page.getByLabel("Allele", { exact: true });
    await allele.waitFor({ timeout: 10000 });
    const n = await allele.locator("option").count();
    if (n < 1) throw new Error("no alleles listed");
    await allele.selectOption({ index: Math.min(1, n - 1) });
    await page.waitForTimeout(800);
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
    const box = await page.locator("canvas.map").boundingBox();
    // A box holding more animals than the Individual-mode cap is refused; try smaller ones.
    for (const size of [0.12, 0.06, 0.03, 0.015]) {
      await page.getByRole("button", { name: /Watch region/ }).click();
      await page.mouse.move(box.x + box.width * 0.3, box.y + box.height * 0.3);
      await page.mouse.down();
      await page.mouse.move(box.x + box.width * (0.3 + size), box.y + box.height * (0.3 + size * 1.25), { steps: 8 });
      await page.mouse.up();
      await page.waitForTimeout(1500);
      if (/region/.test((await page.locator(".badge").textContent()) ?? "")) break;
    }
    const badge = await page.locator(".badge").textContent();
    if (!/region/.test(badge ?? "")) throw new Error(`badge after watch region: ${badge}`);
  });
  await step("a jump ends the region and restores the mode choice", async () => {
    await page.getByRole("button", { name: "+100 yr" }).click();
    // The overlay may not have appeared yet, so wait for the result rather than for it to close.
    await page
      .locator('[aria-label="Fidelity mode"] [aria-checked="true"]', { hasText: "Auto" })
      .waitFor({ timeout: 60000 })
      .catch(() => {
        throw new Error("mode choice stayed locked to Individual after the jump");
      });
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
  await step("export data files", async () => {
    await page.getByRole("tab", { name: "World" }).click();
    const download = async (name) => {
      const [d] = await Promise.all([page.waitForEvent("download"), page.getByRole("button", { name }).click()]);
      const file = join(tmpdir(), `evo-export-${process.pid}-${d.suggestedFilename()}`);
      await d.saveAs(file);
      return { name: d.suggestedFilename(), text: readFileSync(file, "utf8") };
    };
    const first = page.getByLabel("Species to export").locator("option").nth(1);
    await page.getByLabel("Species to export").selectOption({ value: await first.getAttribute("value") });
    const series = await download("Time series (CSV)");
    if (!/^species_\d+_.+\.csv$/.test(series.name) || !series.text.startsWith("year,n,het,body_mass,")) throw new Error(`bad time series ${series.name}`);
    const chronicle = await download("Chronicle (CSV)");
    if (!chronicle.text.startsWith("year,kind,species,species_name,text\n")) throw new Error("bad chronicle header");
    const tree = await download("Phylogeny (Newick)");
    if (!/^\(.*\);$/.test(tree.text)) throw new Error("bad Newick");
    const settings = JSON.parse((await download("Settings (JSON)")).text);
    if (settings.format !== "evolutions-settings" || settings.template !== "earth") throw new Error("bad settings");
  });
  await step("export a replay log and replay it", async () => {
    // The log survived the autosave, so the resumed world can be replayed from its seed.
    await page.getByRole("tab", { name: "World" }).click();
    const [download] = await Promise.all([page.waitForEvent("download"), page.getByRole("button", { name: "Export replay log" }).click()]);
    const file = join(tmpdir(), `evo-replay-${process.pid}.json`);
    await download.saveAs(file);
    await page.locator("header input[type=file]").setInputFiles(file);
    await page.getByText("Replaying…").waitFor();
    await page.getByText(/identical to the original run/).waitFor({ timeout: 300000 });
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
