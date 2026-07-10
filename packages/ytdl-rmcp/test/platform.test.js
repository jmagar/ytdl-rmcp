"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const {
  downloadUrl,
  releaseVersion,
  targetFor,
} = require("../lib/platform");
const { version: packageVersion } = require("../package.json");

test("maps supported platforms to release assets", () => {
  assert.deepEqual(targetFor("linux", "x64"), {
    asset: "ytdl-rmcp-x86_64.tar.gz",
    binary: "rytdl",
  });
  assert.deepEqual(targetFor("win32", "x64"), {
    asset: "ytdl-rmcp-windows-x86_64.tar.gz",
    binary: "rytdl.exe",
  });
});

test("rejects unsupported platforms", () => {
  assert.throws(() => targetFor("darwin", "arm64"), /Unsupported platform/);
});

test("uses npm package version as the binary tag by default", () => {
  assert.equal(releaseVersion({}), `v${packageVersion}`);
});

test("allows release tag override", () => {
  const env = {
    YTDL_RMCP_BINARY_VERSION: "v9.9.9",
    YTDL_RMCP_RELEASE_BASE_URL: "https://example.test/releases",
  };
  assert.equal(
    downloadUrl(targetFor("linux", "x64"), env),
    "https://example.test/releases/v9.9.9/ytdl-rmcp-x86_64.tar.gz",
  );
});
