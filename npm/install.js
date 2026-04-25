#!/usr/bin/env node

"use strict";

const crypto = require("crypto");
const fs = require("fs");
const os = require("os");
const path = require("path");
const { Readable } = require("stream");
const { pipeline } = require("stream/promises");
const { createWriteStream, mkdirSync, rmSync } = require("fs");
const { spawnSync } = require("child_process");
const { getPlatform } = require("./platform");

const INSTALL_DIR = path.join(__dirname, "bin");
const REPOSITORY = "davegomez/dbx-cli";

function getDownloadUrl(artifactName) {
  const { version } = require("./package.json");
  return `https://github.com/${REPOSITORY}/releases/download/v${version}/${artifactName}`;
}

function sanitize(value) {
  return String(value)
    .replace(/\x1b(?:[@-Z\\-_]|\[[0-?]*[ -/]*[@-~]|\][^\x07]*(?:\x07|\x1b\\))/g, "")
    .replace(/[\x00-\x08\x0b\x0c\x0e-\x1f\x7f]/g, "");
}

async function download(url, dest) {
  const response = await fetch(url, { redirect: "follow" });

  if (!response.ok) {
    throw new Error(`Failed to download ${url}: ${response.status} ${response.statusText}`);
  }
  if (!response.body) {
    throw new Error(`Failed to download ${url}: response body is empty`);
  }

  await pipeline(Readable.fromWeb(response.body), createWriteStream(dest));
}

function run(command, args) {
  const result = spawnSync(command, args, { stdio: "pipe" });
  if (result.error) {
    throw new Error(`Failed to run ${command}: ${result.error.message}`);
  }
  if ((result.status ?? 1) !== 0) {
    const stderr = result.stderr ? result.stderr.toString() : "";
    throw new Error(`Command failed: ${command} ${args.join(" ")}\n${sanitize(stderr)}`);
  }
  return result;
}

function assertSafeArchiveEntry(entry) {
  const normalized = entry.replace(/\\/g, "/");
  if (
    normalized === "" ||
    normalized.startsWith("/") ||
    normalized.includes(":") ||
    normalized.split("/").includes("..")
  ) {
    throw new Error(`Archive contains unsafe path: ${entry}`);
  }
}

function verifyTarEntries(archivePath) {
  const result = run("tar", ["tf", archivePath]);
  const entries = result.stdout.toString().split(/\r?\n/).filter(Boolean);
  for (const entry of entries) {
    assertSafeArchiveEntry(entry);
  }
}

function verifyZipEntries(archivePath) {
  if (process.platform !== "win32") {
    return;
  }

  const script = [
    "Add-Type -AssemblyName System.IO.Compression.FileSystem",
    `$archive = [System.IO.Compression.ZipFile]::OpenRead('${archivePath.replace(/'/g, "''")}')`,
    "try { $archive.Entries | ForEach-Object { $_.FullName } } finally { $archive.Dispose() }",
  ].join("; ");
  const result = run("powershell.exe", ["-NoProfile", "-NonInteractive", "-Command", script]);
  const entries = result.stdout.toString().split(/\r?\n/).filter(Boolean);
  for (const entry of entries) {
    assertSafeArchiveEntry(entry);
  }
}

function extract(archivePath, destDir) {
  if (archivePath.endsWith(".tar.gz")) {
    verifyTarEntries(archivePath);
    run("tar", ["xzf", archivePath, "-C", destDir]);
    return;
  }

  if (archivePath.endsWith(".zip")) {
    verifyZipEntries(archivePath);
    if (process.platform !== "win32") {
      throw new Error("Zip extraction is only supported on Windows");
    }

    const psArchive = archivePath.replace(/'/g, "''");
    const psDest = destDir.replace(/'/g, "''");
    run("powershell.exe", [
      "-NoProfile",
      "-NonInteractive",
      "-Command",
      `Expand-Archive -LiteralPath '${psArchive}' -DestinationPath '${psDest}' -Force`,
    ]);
    return;
  }

  throw new Error(`Unsupported archive format: ${archivePath}`);
}

function verifyChecksum(archivePath, checksumPath) {
  const expectedHash = fs.readFileSync(checksumPath, "utf8").trim().split(/\s+/)[0].toLowerCase();
  if (!/^[a-f0-9]{64}$/.test(expectedHash)) {
    throw new Error(`Invalid SHA256 file: ${checksumPath}`);
  }

  const actualHash = crypto.createHash("sha256").update(fs.readFileSync(archivePath)).digest("hex");
  if (actualHash !== expectedHash) {
    throw new Error(
      `SHA256 checksum mismatch\nExpected: ${expectedHash}\nActual:   ${actualHash}`,
    );
  }
}

async function install() {
  const platform = getPlatform();
  const { version } = require("./package.json");
  const binaryPath = path.join(INSTALL_DIR, platform.binary);
  const versionFile = path.join(INSTALL_DIR, ".version");

  if (fs.existsSync(binaryPath) && fs.existsSync(versionFile)) {
    const installedVersion = fs.readFileSync(versionFile, "utf8").trim();
    if (installedVersion === version) {
      console.error(`dbx v${version} already installed, skipping.`);
      return;
    }
    console.error(`Upgrading dbx from v${installedVersion} to v${version}`);
  }

  rmSync(INSTALL_DIR, { recursive: true, force: true });
  mkdirSync(INSTALL_DIR, { recursive: true });

  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "dbx-cli-"));
  const archiveName = path.basename(platform.artifact);
  const archivePath = path.join(tmpDir, archiveName);
  const checksumPath = `${archivePath}.sha256`;
  const archiveUrl = getDownloadUrl(platform.artifact);
  const checksumUrl = `${archiveUrl}.sha256`;

  try {
    console.error(`Downloading dbx from ${archiveUrl}`);
    await download(archiveUrl, archivePath);

    console.error(`Verifying checksum from ${checksumUrl}`);
    await download(checksumUrl, checksumPath);
    verifyChecksum(archivePath, checksumPath);
    console.error("Checksum verified");

    console.error(`Extracting to ${INSTALL_DIR}`);
    extract(archivePath, INSTALL_DIR);

    if (!fs.existsSync(binaryPath)) {
      throw new Error(`Archive did not contain expected binary: ${platform.binary}`);
    }
    if (process.platform !== "win32") {
      fs.chmodSync(binaryPath, 0o755);
    }

    fs.writeFileSync(versionFile, `${version}\n`);
    console.error(`dbx v${version} installed`);
  } finally {
    rmSync(tmpDir, { recursive: true, force: true });
  }
}

install().catch((error) => {
  console.error(`Error installing dbx: ${sanitize(error.message)}`);
  process.exit(1);
});

module.exports = { getDownloadUrl, install, sanitize, verifyChecksum };
