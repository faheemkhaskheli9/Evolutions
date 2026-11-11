// End-to-end smoke test: build output served by `vite preview`, driven in Chromium.
// Usage: npm run build && npm run test:e2e   (set SHOTS=dir to save screenshots)
import { spawn } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
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
  await step("rewind to a checkpoint repeats the run", async () => {
    const hash = () => page.evaluate(() => window.__evoHash());
    const jump100 = async () => {
      const target = (await hash()).year + 100;
      await page.getByRole("button", { name: "+100 yr" }).click();
      // The overlay may come and go between checks, so wait for the year instead.
      for (let t = 0; (await hash()).year < target; t++) {
        if (t > 600) throw new Error("jump did not finish");
        await page.waitForTimeout(200);
      }
      await page.getByText("Fast-forwarding…").waitFor({ state: "detached", timeout: 30000 });
    };
    await jump100();
    const original = await hash();
    // The checkpoint taken just before that jump.
    const marks = page.getByRole("button", { name: /^Rewind to year .* \(before a jump\)$/ });
    await marks.last().click();
    await page.getByText(/^Rewound to year/).waitFor({ timeout: 20000 });
    const back = await hash();
    if (!(back.year < original.year - 99)) throw new Error(`rewound to year ${back.year}, ran to ${original.year}`);
    await jump100();
    const again = await hash();
    if (again.year !== original.year || again.hash !== original.hash) throw new Error(`rewound run differs: ${JSON.stringify(again)} vs ${JSON.stringify(original)}`);
  });
  await step("species panel shows history", async () => {
    await page.getByRole("tab", { name: "Species" }).click();
    await page.getByText("Population", { exact: true }).first().waitFor();
  });
  await step("why did a trait change: selection or drift", async () => {
    await page.getByRole("button", { name: /^Why did .* change\?/ }).click();
    await page.locator(".trait-why .why-verdict").waitFor({ timeout: 20000 });
    const text = await page.locator(".trait-why .why").innerText();
    if (!/drift/i.test(text)) throw new Error(`trait explanation: ${text}`);
  });
  await step("tree and chronicle render", async () => {
    await page.getByRole("tab", { name: "Tree" }).click();
    await page.locator("svg.phylo").waitFor();
    await page.getByRole("tab", { name: /Chronicle/ }).click();
    await page.locator(".events li").first().waitFor();
  });
  await step("why? on a chronicle event", async () => {
    await page.locator(".ev-why").first().click();
    await page.locator(".events .why p").first().waitFor({ timeout: 20000 });
    const text = await page.locator(".events .why").first().innerText();
    if (text.length < 40) throw new Error(`event explanation: ${text}`);
    // The chart link leaves the Chronicle for the Species panel or the tree.
    await page.locator(".events .why button.link").first().click();
    await page.locator('[role="tab"][aria-selected="true"]', { hasText: /Species|Tree/ }).waitFor();
    await page.getByRole("tab", { name: /Chronicle/ }).click();
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
  await step("macro mode for deep time", async () => {
    await page.getByRole("radio", { name: "Macro", exact: true }).click();
    await page.locator(".badge", { hasText: "Macro" }).waitFor({ timeout: 10000 });
    const hash = () => page.evaluate(() => window.__evoHash());
    const target = (await hash()).year + 10000;
    await page.getByRole("button", { name: "+10k yr" }).click();
    for (let t = 0; (await hash()).year < target - 1e-6; t++) {
      if (t > 900) throw new Error("10,000 years in Macro mode took too long");
      await page.waitForTimeout(200);
    }
    // Back to Auto at a slow speed: Macro mode is left again.
    await page.getByRole("radio", { name: "Auto", exact: true }).click();
    await page.locator(".badge", { hasText: /Population|Individual/ }).waitFor({ timeout: 10000 });
  });
  await step("climate intervention is logged", async () => {
    await page.getByRole("button", { name: "-2 °C" }).click();
    await page.getByRole("tab", { name: /Chronicle/ }).click();
    await page.getByText(/Climate shift/).first().waitFor();
  });
  await step("go to just before an event", async () => {
    await page.getByRole("button", { name: /^Go to just before: Climate shift/ }).first().click();
    await page.getByText(/^Rewound to year/).waitFor({ timeout: 20000 });
    await page.getByText(/Climate shift/).first().waitFor({ state: "detached", timeout: 5000 });
    // Apply it again so the later steps see it.
    await page.getByRole("tab", { name: "World" }).click();
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
  let templateFile;
  let strait;
  await step("save the world as a template", async () => {
    await page.getByRole("tab", { name: "World" }).click();
    await page.getByRole("button", { name: "Save as template…" }).click();
    await page.getByLabel("Template name").fill("Smoke lesson");
    await page.getByLabel("Template description").fill("Hares, deer and wolves on a continent.");
    const [d] = await Promise.all([page.waitForEvent("download"), page.getByRole("button", { name: "Save template" }).click()]);
    if (d.suggestedFilename() !== "smoke-lesson.evotemplate") throw new Error(`bad template file name ${d.suggestedFilename()}`);
    templateFile = join(tmpdir(), `evo-template-${process.pid}.evotemplate`);
    await d.saveAs(templateFile);
    const t = JSON.parse(readFileSync(templateFile, "utf8"));
    if (t.format !== "evolutions-template" || t.name !== "Smoke lesson" || t.base !== "earth" || t.founders.length !== 3) throw new Error("bad template file");
  });
  await step("a tampered template is rejected", async () => {
    const t = JSON.parse(readFileSync(templateFile, "utf8"));
    t.founders[0].count = 0;
    const bad = join(tmpdir(), `evo-template-bad-${process.pid}.evotemplate`);
    writeFileSync(bad, JSON.stringify(t));
    await page.getByRole("button", { name: "New world" }).click();
    await page.getByLabel("Import template").setInputFiles(bad);
    await page.getByText(/Could not import .*founder count must be 1\.\.20000/).waitFor();
    await page.getByRole("button", { name: "Close" }).click();
  });
  await step("a template survives a reload and recreates its world", async () => {
    await page.reload();
    await page.getByText("Create a world").waitFor({ timeout: 20000 });
    const seed = JSON.parse(readFileSync(templateFile, "utf8")).seed;
    // Importing the downloaded file again keeps one copy.
    await page.getByLabel("Import template").setInputFiles(templateFile);
    await page.getByText(/Template "Smoke lesson" saved/).waitFor();
    if ((await page.getByRole("button", { name: /^Smoke lesson/ }).count()) !== 1) throw new Error("template listed twice");
    await page.getByRole("button", { name: /^Smoke lesson/ }).click();
    if ((await page.getByLabel("Seed", { exact: true }).inputValue()) !== String(seed)) throw new Error("template seed not applied");
    await page.getByRole("button", { name: "Create world" }).click();
    await page.locator(".badge").waitFor({ timeout: 30000 });
    const fromTemplate = await page.evaluate(() => window.__evoHash());
    // The same world made from the built-in template with the same seed.
    await page.getByRole("button", { name: "New world" }).click();
    await page.getByRole("button", { name: /Earth-like Continent/ }).click();
    await page.getByLabel("Seed", { exact: true }).fill(String(seed));
    await page.getByRole("button", { name: "Create world" }).click();
    await page.getByText("Create a world").waitFor({ state: "detached", timeout: 30000 });
    const direct = await page.evaluate(() => window.__evoHash());
    if (fromTemplate.year !== 0 || fromTemplate.hash !== direct.hash) throw new Error(`template world differs: ${JSON.stringify([fromTemplate, direct])}`);
  });
  await step("paint a sea, undo and redo it, and keep it in a template", async () => {
    const land = () => page.evaluate(() => window.__evoLand());
    const hash = async () => (await page.evaluate(() => window.__evoHash())).hash;
    const before = await land();
    const h0 = await hash();
    await page.getByRole("tab", { name: "World" }).click();
    await page.getByRole("button", { name: "✎ Edit world" }).click();
    await page.getByRole("radio", { name: "Sea" }).click();
    await page.getByLabel("Brush size").fill("6");
    // A stroke across the middle of the continent.
    const at = (tx, ty) => page.evaluate(([x, y]) => window.__evoMap.toScreen(x, y), [tx, ty]);
    const [ax, ay] = await at(40, 32);
    const [bx, by] = await at(88, 32);
    await page.mouse.move(ax, ay);
    await page.mouse.down();
    await page.mouse.move(bx, by, { steps: 12 });
    await page.mouse.up();
    await page.waitForFunction((n) => window.__evoLand() < n, before, { timeout: 15000 });
    const after = await land();
    const h1 = await hash();
    await page.getByRole("tab", { name: /Chronicle/ }).click();
    await page.getByText(/World edited: painted sea/).first().waitFor();
    await page.getByRole("tab", { name: "World" }).click();
    await page.getByRole("button", { name: "↶ Undo" }).click();
    await page.waitForFunction((n) => window.__evoLand() === n, before, { timeout: 15000 });
    if ((await hash()) !== h0) throw new Error("undo did not restore the world");
    await page.keyboard.press("Control+Shift+Z");
    await page.waitForFunction((n) => window.__evoLand() === n, after, { timeout: 15000 });
    if ((await hash()) !== h1) throw new Error("redo gave a different world");
    await page.getByRole("button", { name: "✓ Done editing" }).click();
    await page.getByRole("button", { name: "Save as template…" }).click();
    await page.getByLabel("Template name").fill("Smoke strait");
    const [d] = await Promise.all([page.waitForEvent("download"), page.getByRole("button", { name: "Save template" }).click()]);
    const file = join(tmpdir(), `evo-template-edit-${process.pid}.evotemplate`);
    await d.saveAs(file);
    const t = JSON.parse(readFileSync(file, "utf8"));
    if (t.version !== 2 || t.edits?.length !== 1 || t.edits[0].brush !== "Ocean") throw new Error(`template lacks the edit: ${JSON.stringify(t.edits)}`);
    console.log(`(land ${before} → ${after} tiles) `);
    strait = { file, land: after };
  });
  await step("a template with edits recreates the painted world", async () => {
    await page.getByRole("button", { name: "New world" }).click();
    await page.getByLabel("Import template").setInputFiles(strait.file);
    await page.getByText(/Template "Smoke strait" saved/).waitFor();
    await page.getByRole("button", { name: /^Smoke strait/ }).click();
    await page.getByRole("button", { name: "Create world" }).click();
    await page.getByRole("heading", { name: "Create a world" }).waitFor({ state: "detached", timeout: 30000 });
    await page.waitForFunction((n) => window.__evoLand() === n, strait.land, { timeout: 15000 });
  });
  await step("design a species: impossible designs are refused, possible ones saved", async () => {
    await page.getByRole("button", { name: "New world" }).click();
    await page.getByRole("button", { name: /Earth-like Continent/ }).click();
    await page.getByRole("button", { name: "✎ Design a species…" }).click();
    await page.getByLabel("Species name").fill("Snow grazer");
    // Every costly trait at its maximum: the energy budget cannot balance.
    for (const l of ["Relative brain size (EQ)", "Somatic maintenance", "Insulation (fur/fat)", "Ornament size", "Sensory acuity", "Muscle: sprint vs endurance", "Kidney concentrating ability", "Evaporative cooling capacity"])
      await page.getByLabel(l, { exact: true }).fill("1000");
    await page.getByText(/Rejected: Impossible energy budget/).waitFor();
    if (await page.getByRole("button", { name: "Save species" }).isEnabled()) throw new Error("an impossible design can be saved");
    await page.getByText(/Can't save: Impossible energy budget/).waitFor();
    // Back to hare values, with a thick coat and a bigger body: possible.
    await page.getByLabel("Start from").selectOption({ label: "Steppe hare" });
    await page.getByLabel("Insulation (fur/fat)", { exact: true }).fill("900");
    await page.getByLabel("Adult body mass", { exact: true }).fill("400");
    await page.getByText(/Rejected/).waitFor({ state: "detached" });
    await page.getByRole("button", { name: "Save species" }).click();
    await page.getByRole("dialog", { name: "Design a species" }).waitFor({ state: "detached" });
    // The new species is added as a founder group.
    const rows = page.getByLabel("Founder species");
    const last = rows.nth((await rows.count()) - 1);
    if ((await last.inputValue()) !== "my-snow-grazer") throw new Error(`founder row not added: ${await last.inputValue()}`);
    await page.getByRole("button", { name: "Create world" }).click();
    await page.getByRole("heading", { name: "Create a world" }).waitFor({ state: "detached", timeout: 30000 });
    await page.getByRole("tab", { name: /Chronicle/ }).click();
    await page.getByText(/founders of Snow grazer .* released/).first().waitFor();
  });
  await step("a designed species travels in templates and can be introduced later", async () => {
    await page.getByRole("tab", { name: "World" }).click();
    await page.getByRole("button", { name: "Save as template…" }).click();
    await page.getByLabel("Template name").fill("Snow lesson");
    const [d] = await Promise.all([page.waitForEvent("download"), page.getByRole("button", { name: "Save template" }).click()]);
    const file = join(tmpdir(), `evo-template-design-${process.pid}.evotemplate`);
    await d.saveAs(file);
    const t = JSON.parse(readFileSync(file, "utf8"));
    if (t.version !== 3 || t.designs?.length !== 1 || t.designs[0].name !== "Snow grazer" || !(t.designs[0].traits.insulation > 0.85)) throw new Error(`template lacks the design: ${JSON.stringify(t.designs)}`);
    // Introduce it again, where it will do best.
    await page.getByLabel("Species to introduce").selectOption({ label: "Snow grazer" });
    await page.getByRole("button", { name: "Best habitat" }).click();
    await page.getByRole("tab", { name: /Chronicle/ }).click();
    await page.waitForFunction(() => [...document.querySelectorAll(".ev")].filter((e) => /founders of Snow grazer/.test(e.textContent ?? "")).length >= 2, null, { timeout: 15000 });
    // The world, saved and opened again, still knows the design.
    await page.getByRole("tab", { name: "World" }).click();
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
