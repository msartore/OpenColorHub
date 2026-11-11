#!/usr/bin/env node
// Derives the application version from a git tag and writes it into every
// manifest that carries one, so cutting a release is `git tag vX.Y.Z` and
// nothing else.
//
// Two places: the Rust workspace (core + CLI) and the Native Shell app
// (`nsh/nsh.json` and its own crate). The NSH package and `update-manifest`
// both read `nsh/nsh.json`, so the installer and the update feed agree with the
// tag.
//
// Usage: node scripts/set-version.mjs v1.2.3

import { readFileSync, writeFileSync } from "node:fs";

const tag = process.argv[2] ?? "";
const version = tag.trim().replace(/^v/, "");

if (!/^\d+\.\d+\.\d+$/.test(version)) {
  console.error(`set-version: expected a tag like v1.2.3, got "${tag}"`);
  process.exit(1);
}

const changes = [];

function edit(path, re, label) {
  const before = readFileSync(path, "utf8");
  let hits = 0;
  const after = before.replace(re, (match, current) => {
    hits += 1;
    return current === version ? match : match.replace(current, version);
  });
  if (!hits) {
    console.error(`set-version: ${path} has no ${label} to rewrite`);
    process.exit(1);
  }
  if (after !== before) {
    writeFileSync(path, after);
    changes.push(`${path} (${hits})`);
  }
}

// [workspace.package] holds the version the core and CLI inherit.
{
  const path = "Cargo.toml";
  const text = readFileSync(path, "utf8");
  const start = text.indexOf("[workspace.package]");
  if (start < 0) {
    console.error("set-version: Cargo.toml has no [workspace.package]");
    process.exit(1);
  }
  const match = /version = "([^"]+)"/.exec(text.slice(start));
  if (!match) {
    console.error("set-version: no version in [workspace.package]");
    process.exit(1);
  }
  if (match[1] !== version) {
    const at = start + match.index;
    const updated = `${text.slice(0, at)}version = "${version}"${text.slice(at + match[0].length)}`;
    writeFileSync(path, updated);
    changes.push(`${path} (${match[1]} -> ${version})`);
  }
}

edit("nsh/nsh.json", /"version":\s*"([^"]+)"/, "version");
edit("nsh/Cargo.toml", /^version = "([^"]+)"/m, "package version");

if (changes.length) {
  console.log(`version ${version} applied to:`);
  for (const change of changes) console.log(`  ${change}`);
} else {
  console.log(`version ${version} already in place`);
}
