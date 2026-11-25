// Screenshots of the site, and its islands at work: `node tests/browser/site.cjs http://localhost:3400 out/`.
const { chromium } = require("playwright");
(async () => {
  const base = process.argv[2] || "http://localhost:3000";
  const out = process.argv[3] || "/tmp";
  const browser = await chromium.launch({ args: ["--use-gl=angle", "--use-angle=swiftshader", "--enable-unsafe-swiftshader"] });
  const errors = [];
  const shot = async (name, width, height, path, steps) => {
    const page = await browser.newPage({ viewport: { width, height }, deviceScaleFactor: 1 });
    page.on("console", (m) => { if (m.type() === "error" || m.type() === "warning") errors.push(`${name}: ${m.text().slice(0, 300)}`); });
    page.on("pageerror", (e) => errors.push(`${name}: ${e}`));
    await page.goto(base + path, { waitUntil: "networkidle" });
    await page.waitForTimeout(800);
    for (const [i, y] of steps.entries()) {
      await page.evaluate((y) => window.scrollTo({ top: y, behavior: "instant" }), y);
      await page.waitForTimeout(1300);
      await page.screenshot({ path: `${out}/${name}-${i}.png` });
    }
    return page;
  };
  const home = await shot("home", 1440, 900, "/", [0, 900, 1800, 2700, 3600, 4500, 5400, 6300, 7200, 8100]);
  // The islands.
  await home.evaluate(() => document.querySelector("#essayer").scrollIntoView({ behavior: "instant" }));
  await home.waitForSelector("facile-island[data-component=essai][data-live]", { state: "attached", timeout: 20000 });
  await home.click(".essai .lancer");
  await home.waitForFunction(() => !document.querySelector(".sortie").textContent.startsWith("▶"));
  const sortie = await home.textContent(".sortie");
  await home.click("button[aria-label=plus]");
  await home.click("button[aria-label=plus]");
  const compteur = await home.textContent(".compteur .valeur");
  await home.locator("#essayer").scrollIntoViewIfNeeded();
  await home.waitForTimeout(600);
  await home.screenshot({ path: `${out}/home-islands.png` });
  await shot("mobile", 390, 844, "/", [0, 800, 1600, 2600, 3600]);
  await shot("docs", 1440, 900, "/docs/orm", [0, 900]);
  await shot("bench", 1440, 900, "/benchmark", [0]);
  await shot("footer", 1440, 900, "/en/", [100000]);
  await shot("404", 1440, 900, "/nulle-part", [0]);
  console.log(JSON.stringify({ sortie: sortie.slice(0, 200), compteur, errors }, null, 1));
  await browser.close();
})();
