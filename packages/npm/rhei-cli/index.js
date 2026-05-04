"use strict";

const childProcess = require("child_process");
const fs = require("fs");
const path = require("path");

const VERSION = require("./package.json").version;
const EXE = process.platform === "win32" ? "rhei.exe" : "rhei";

function installRoot() {
  return process.env.RHEI_CLI_INSTALL_DIR || path.join(__dirname, "vendor");
}

function binaryPath() {
  return path.join(installRoot(), "bin", EXE);
}

function ensureBinarySync(options = {}) {
  const bin = binaryPath();
  if (fs.existsSync(bin)) {
    return bin;
  }

  const args = [
    "install",
    "rhei-cli",
    "--version",
    VERSION,
    "--root",
    installRoot(),
    "--locked",
    "--force"
  ];

  if (!options.quiet) {
    console.error(`Installing rhei-cli ${VERSION} with Cargo...`);
  }

  const result = childProcess.spawnSync("cargo", args, {
    stdio: options.quiet ? "pipe" : "inherit",
    env: { ...process.env, CARGO_TERM_COLOR: process.env.CARGO_TERM_COLOR || "always" }
  });

  if (result.error) {
    throw new Error(
      `failed to run cargo: ${result.error.message}. Install Rust/Cargo from https://rustup.rs, then reinstall rhei-cli.`
    );
  }
  if (result.status !== 0) {
    throw new Error(`cargo install rhei-cli exited with status ${result.status}`);
  }
  if (!fs.existsSync(bin)) {
    throw new Error(`cargo install completed, but ${bin} was not created`);
  }

  return bin;
}

function runSync(args = [], options = {}) {
  const bin = ensureBinarySync({ quiet: options.quiet });
  return childProcess.spawnSync(bin, args, {
    stdio: options.stdio || "inherit",
    cwd: options.cwd,
    env: options.env || process.env
  });
}

function runCaptureSync(args = [], options = {}) {
  const bin = ensureBinarySync({ quiet: options.quiet !== false });
  return childProcess.spawnSync(bin, args, {
    cwd: options.cwd,
    env: options.env || process.env,
    encoding: options.encoding || "utf8"
  });
}

function run(args = [], options = {}) {
  return new Promise((resolve, reject) => {
    let bin;
    try {
      bin = ensureBinarySync({ quiet: options.quiet });
    } catch (error) {
      reject(error);
      return;
    }

    const child = childProcess.spawn(bin, args, {
      stdio: options.stdio || "inherit",
      cwd: options.cwd,
      env: options.env || process.env
    });
    child.on("error", reject);
    child.on("exit", (code, signal) => resolve({ code, signal }));
  });
}

module.exports = {
  VERSION,
  binaryPath,
  ensureBinarySync,
  run,
  runSync,
  runCaptureSync
};
