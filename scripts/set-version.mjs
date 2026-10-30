#!/usr/bin/env node
// Derives the application version from a git tag and writes it into every
// manifest that carries one, so cutting a release is `git tag vX.Y.Z` and
// nothing else.
//
// The tag is the single source of truth. Keeping four manifests in sync by hand
// is exactly the kind of step that gets forgotten, and the failure mode is a
// release whose installer says 0.2.0 while its tag says 0.3.0 — which then makes
// the self-updater offer an update forever, because the running build reports a
// version lower than the release it was cut from.
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

// [workspace.package] holds the version the crates inherit.
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

edit("src-tauri/tauri.conf.json", /"version":\s*"([^"]+)"/, "version");
edit("package.json", /"version":\s*"([^"]+)"/, "version");

// package-lock carries the version twice: once at the root, once for the root
// package entry. Both are rewritten, nothing else in the lock has this shape.
{
  const path = "package-lock.json";
  const text = readFileSync(path, "utf8");
  const updated = text.replace(/"version":\s*"(\d+\.\d+\.\d+)"/g, (match, current) =>
    current === version ? match : match.replace(current, version),
  );
  if (updated !== text) {
    writeFileSync(path, updated);
    changes.push(`${path}`);
  }
}

if (changes.length) {
  console.log(`version ${version} applied to:`);
  for (const change of changes) console.log(`  ${change}`);
} else {
  console.log(`version ${version} already in place`);
}
