import { defineConfig, type Plugin } from "vite";
import { resolve } from "path";
import { readdirSync, readFileSync } from "fs";
import { DISCORD_URL } from "../src/community";

const SITE_URL = "https://tuicommander.com";

// Collect every HTML file in a directory as additional entry points
function htmlEntries(dir: string) {
  return Object.fromEntries(
    readdirSync(resolve(__dirname, dir))
      .filter((f) => f.endsWith(".html"))
      .map((f) => [`${dir}/${f.replace(".html", "")}`, resolve(__dirname, dir, f)]),
  );
}

const input = {
  main: resolve(__dirname, "index.html"),
  "2.0": resolve(__dirname, "2.0.html"),
  download: resolve(__dirname, "download.html"),
  blog: resolve(__dirname, "blog.html"),
  ...htmlEntries("features"),
  ...htmlEntries("blog"),
};

// KEY=value lines; film.env is also sourced by .github/workflows/website.yml
const film = Object.fromEntries(
  readFileSync(resolve(__dirname, "film.env"), "utf8")
    .split("\n")
    .filter((l) => /^[A-Z_]+=/.test(l))
    .map((l) => l.split("=", 2)),
);

// `<!-- @include nav -->` → partials/nav.html, so the pages share one nav and footer
function partials(): Plugin {
  return {
    name: "partials",
    transformIndexHtml: {
      order: "pre",
      handler: (html, { filename }) =>
        html
          .replace(/<!-- @include ([a-z-]+) -->/g, (_, name) =>
            readFileSync(resolve(__dirname, "partials", `${name}.html`), "utf8"),
          )
          .replaceAll("%DISCORD_URL%", DISCORD_URL)
          .replaceAll(
            "%FILM_SRC%",
            filename === input.main ? film.HOME_FILM_URL : `/video/${film.FILM_ASSET}`,
          ),
    },
  };
}

// Every page plus every published post; the post list comes from blog/index.json
function sitemap(): Plugin {
  return {
    name: "sitemap",
    apply: "build",
    generateBundle() {
      const pages = Object.keys(input)
        .filter((k) => !k.startsWith("blog/"))
        .map((k) => (k === "main" ? "/" : `/${k}.html`));
      const posts = JSON.parse(readFileSync(resolve(__dirname, "blog/index.json"), "utf8"))
        .posts.filter((p: { status: string }) => p.status === "published")
        .map((p: { slug: string; date: string }) => ({ path: `/blog/${p.slug}.html`, date: p.date }));
      const urls = [
        ...pages.map((path) => `  <url><loc>${SITE_URL}${path}</loc></url>`),
        ...posts.map(
          (p: { path: string; date: string }) =>
            `  <url><loc>${SITE_URL}${p.path}</loc><lastmod>${p.date}</lastmod></url>`,
        ),
      ];
      this.emitFile({
        type: "asset",
        fileName: "sitemap.xml",
        source: `<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n${urls.join("\n")}\n</urlset>\n`,
      });
    },
  };
}

export default defineConfig({
  base: "/",
  plugins: [partials(), sitemap()],
  build: {
    outDir: "dist",
    rollupOptions: { input },
  },
});
