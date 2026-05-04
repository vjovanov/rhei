#!/usr/bin/env node
"use strict";

const { runSync } = require("../index");

try {
  const result = runSync(process.argv.slice(2), { quiet: true });
  if (result.error) {
    console.error(result.error.message);
    process.exit(1);
  }
  if (result.signal) {
    process.kill(process.pid, result.signal);
  }
  process.exit(result.status === null ? 1 : result.status);
} catch (error) {
  console.error(error.message);
  process.exit(1);
}
