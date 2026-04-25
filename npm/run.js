#!/usr/bin/env node

"use strict";

const fs = require("fs");
const path = require("path");
const { spawnSync } = require("child_process");
const { getPlatform } = require("./platform");

function sanitize(value) {
  return String(value)
    .replace(/\x1b(?:[@-Z\\-_]|\[[0-?]*[ -/]*[@-~]|\][^\x07]*(?:\x07|\x1b\\))/g, "")
    .replace(/[\x00-\x08\x0b\x0c\x0e-\x1f\x7f]/g, "");
}

const platform = getPlatform();
const binaryPath = path.join(__dirname, "bin", platform.binary);

if (!fs.existsSync(binaryPath)) {
  console.error(`dbx binary not found at ${binaryPath}; auto-installing...`);
  const install = spawnSync(process.execPath, [path.join(__dirname, "install.js")], {
    cwd: __dirname,
    stdio: "inherit",
  });
  if (install.error) {
    console.error(`Error installing dbx: ${sanitize(install.error.message)}`);
    process.exit(1);
  }
  if ((install.status ?? 1) !== 0) {
    process.exit(install.status ?? 1);
  }
}

const result = spawnSync(binaryPath, process.argv.slice(2), {
  cwd: process.cwd(),
  stdio: "inherit",
});

if (result.error) {
  console.error(`Error running dbx: ${sanitize(result.error.message)}`);
  process.exit(1);
}

if (result.signal) {
  process.kill(process.pid, result.signal);
}

process.exit(result.status ?? 1);
