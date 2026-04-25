#!/usr/bin/env node

"use strict";

const os = require("os");
const { spawnSync } = require("child_process");

const { supportedPlatforms } = require("./package.json");

function detectLinuxLibc() {
  if (process.report && typeof process.report.getReport === "function") {
    const report = process.report.getReport();
    if (report.header && report.header.glibcVersionRuntime) {
      return "gnu";
    }
  }

  const result = spawnSync("ldd", ["--version"], {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  });
  const output = `${result.stdout || ""}${result.stderr || ""}`.toLowerCase();
  if (output.includes("musl")) {
    return "musl";
  }

  return "gnu";
}

function getPlatformKey() {
  const rawOs = os.type();
  const rawArch = os.arch();

  let arch;
  switch (rawArch) {
    case "x64":
      arch = "x86_64";
      break;
    case "arm64":
      arch = "aarch64";
      break;
    default:
      throw new Error(`Unsupported architecture: ${rawArch}`);
  }

  let osTarget;
  switch (rawOs) {
    case "Darwin":
      osTarget = "apple-darwin";
      break;
    case "Linux":
      osTarget = `unknown-linux-${detectLinuxLibc()}`;
      break;
    case "Windows_NT":
      osTarget = "pc-windows-msvc";
      break;
    default:
      throw new Error(`Unsupported operating system: ${rawOs}`);
  }

  const key = `${arch}-${osTarget}`;
  if (!supportedPlatforms[key]) {
    throw new Error(
      `Unsupported platform: ${key}\nSupported platforms: ${Object.keys(supportedPlatforms).join(", ")}`,
    );
  }

  return key;
}

function getPlatform() {
  return supportedPlatforms[getPlatformKey()];
}

module.exports = { detectLinuxLibc, getPlatform, getPlatformKey };
