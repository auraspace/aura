import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { initSync, compile } from "./lib/aura.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const WASM_PATH = path.join(__dirname, "lib", "aura_bg.wasm");
const CASES_DIR = path.join(__dirname, "cases");

async function runTest(filePath) {
  const fileName = path.basename(filePath);
  const source = fs.readFileSync(filePath, "utf-8");

  // Extract expected output from comments
  const lines = source.split("\n");
  const expectedOutputLines = [];
  let capture = false;
  for (const line of lines) {
    if (line.includes("// Expected output:")) {
      capture = true;
      continue;
    }
    if (capture) {
      if (line.startsWith("// ")) {
        expectedOutputLines.push(line.replace("// ", "").trim());
      } else {
        break;
      }
    }
  }
  const expectedOutput = expectedOutputLines.join("\n").trim();

  process.stdout.write(`Running ${fileName}... `);

  try {
    const result = compile(source);
    if (!result.ok()) {
      console.log("❌ FAILED (Compilation Error)");
      console.error(result.errors());
      return false;
    }

    const actualOutput = result.output().trim();
    if (actualOutput === expectedOutput) {
      console.log("✅ PASSED");
      return true;
    } else {
      console.log("❌ FAILED (Output Mismatch)");
      console.log("--- Expected ---");
      console.log(expectedOutput);
      console.log("--- Actual ---");
      console.log(actualOutput);
      console.log("----------------");
      return false;
    }
  } catch (err) {
    console.log("❌ FAILED (Panic/Crash)");
    console.error(err);
    return false;
  }
}

async function main() {
  // Initialize WASM
  const wasmBuffer = fs.readFileSync(WASM_PATH);
  initSync({ module: wasmBuffer });

  const filter = process.argv[2];
  let files = fs.readdirSync(CASES_DIR).filter((f) => f.endsWith(".aura"));
  if (filter) {
    files = files.filter((f) => f.includes(filter));
    if (files.length === 0) {
      console.error(`No test cases matching: ${filter}`);
      process.exit(1);
    }
  }
  let passed = 0;
  let failed = 0;

  for (const file of files) {
    const success = await runTest(path.join(CASES_DIR, file));
    if (success) passed++;
    else failed++;
  }

  console.log(`\nSummary: ${passed} passed, ${failed} failed`);
  if (failed > 0) {
    process.exit(1);
  }
}

main().catch((err) => {
  console.error("Test runner failed:", err);
  process.exit(1);
});
