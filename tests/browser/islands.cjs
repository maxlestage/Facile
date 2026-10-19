(async () => {
// Runs the islands of examples/taches in Chromium: `node tests/browser/islands.cjs http://localhost:3300`.
const { chromium } = require("playwright");

const base = process.argv[2] || "http://localhost:3000";
const browser = await chromium.launch();
const page = await browser.newPage();
const errors = [];
page.on("console", (m) => { if (m.type() === "error") errors.push(m.text()); });
page.on("pageerror", (e) => errors.push(String(e)));
page.on("console", (m) => console.log("console:", m.type(), m.text().slice(0, 300)));
await page.goto(base + "/");
await page.waitForSelector("facile-island[data-component=compteur][data-live]", { state: "attached", timeout: 15000 });
const value = () => page.textContent(".compteur .valeur");
const before = await value();
await page.click("button[aria-label=plus]");
await page.click("button[aria-label=plus]");
const after = await value();
await page.click("button[aria-label=moins]");
const after2 = await value();
await page.waitForSelector("facile-island[data-component=liste][data-live]", { state: "attached" });
await page.waitForFunction(() => document.querySelectorAll(".liste li").length > 0);
const n0 = await page.locator(".liste li").count();
await page.fill(".ajout input", "Tester les îlots");
await page.click(".ajout button");
await page.waitForFunction((n) => document.querySelectorAll(".liste li").length === n + 1, n0);
const first = await page.textContent(".liste li");
await page.click(".liste li input[type=checkbox]");
await page.waitForFunction(() => document.querySelector(".liste li").classList.contains("fait"));
const count = await page.textContent(".liste .compte");
console.log(JSON.stringify({ before, after, after2, n0, first, count, errors }));
await browser.close();
if (before !== "3" || after !== "5" || after2 !== "4" || !first.includes("Tester les îlots") || errors.length) process.exit(1);
})();
