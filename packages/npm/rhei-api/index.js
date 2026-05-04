"use strict";

const rheiCli = require("rhei");

function checkedCapture(args) {
  const result = rheiCli.runCaptureSync(args);
  if (result.error) {
    throw result.error;
  }
  if (result.status !== 0) {
    throw new Error((result.stderr || "").trim() || `rhei exited with status ${result.status}`);
  }
  return result.stdout;
}

function version() {
  return checkedCapture(["version"]).trim();
}

function helpText() {
  return checkedCapture(["--help"]);
}

module.exports = {
  version,
  helpText,
  run: rheiCli.run,
  runSync: rheiCli.runSync,
  runCaptureSync: rheiCli.runCaptureSync,
  binaryPath: rheiCli.binaryPath,
  ensureBinarySync: rheiCli.ensureBinarySync
};
