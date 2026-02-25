#!/usr/bin/env node
"use strict";

const { ensureBinarySync } = require("../index");

try {
  ensureBinarySync();
} catch (error) {
  console.error(error.message);
  process.exit(1);
}
