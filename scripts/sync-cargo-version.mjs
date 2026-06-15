#!/usr/bin/env node
import { readFileSync, writeFileSync } from "node:fs";

const packageJson = JSON.parse(readFileSync("package.json", "utf8"));
const version = packageJson.version;

if (!/^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$/.test(version)) {
  throw new Error(`package.json version is not valid semver: ${version}`);
}

const cargoPath = "Cargo.toml";
const cargoToml = readFileSync(cargoPath, "utf8");
const updatedCargoToml = cargoToml.replace(
  /^version = ".*"$/m,
  `version = "${version}"`,
);

if (cargoToml === updatedCargoToml) {
  throw new Error("Cargo.toml package version line was not found");
}

writeFileSync(cargoPath, updatedCargoToml);
console.log(`Synced Cargo.toml package version to ${version}`);
