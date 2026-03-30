import { defineConfig } from "vite";
import { resolve } from "path";
import { readdirSync } from "fs";

// Collect all HTML files in blog/ as additional entry points
const blogDir = resolve(__dirname, "blog");
const blogEntries = Object.fromEntries(
  readdirSync(blogDir)
    .filter((f) => f.endsWith(".html"))
    .map((f) => [`blog/${f.replace(".html", "")}`, resolve(blogDir, f)]),
);

export default defineConfig({
  base: "/",
  build: {
    outDir: "dist",
    rollupOptions: {
      input: {
        main: resolve(__dirname, "index.html"),
        blog: resolve(__dirname, "blog.html"),
        ...blogEntries,
      },
    },
  },
});
