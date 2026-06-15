#!/usr/bin/env node
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";

const packageJson = JSON.parse(readFileSync("package.json", "utf8"));
const tag = `v${packageJson.version}`;

function git(args) {
  return execFileSync("git", args, { encoding: "utf8" }).trim();
}

const status = git(["status", "--porcelain"]);
if (status !== "") {
  throw new Error("Working tree must be clean before creating a release tag");
}

const existingTag = execFileSync("git", ["tag", "--list", tag], {
  encoding: "utf8",
}).trim();

if (existingTag === tag) {
  throw new Error(`Tag ${tag} already exists`);
}

git(["tag", "-a", tag, "-m", `Release ${tag}`]);
git(["push", "origin", tag]);
console.log(`Created and pushed ${tag}`);
