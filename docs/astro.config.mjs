import mdx from "@astrojs/mdx";
import react from "@astrojs/react";
import { mintlify } from "@mintlify/astro";
import { defineConfig } from "astro/config";

export default defineConfig({
  site: "https://dakdevs.github.io",
  base: "/ash",
  integrations: [mintlify({ docsDir: "./content" }), react(), mdx()],
});
