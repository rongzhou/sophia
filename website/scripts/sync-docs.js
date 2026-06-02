const fs = require("node:fs");
const path = require("node:path");

const repoRoot = path.resolve(__dirname, "../..");
const websiteRoot = path.resolve(__dirname, "..");
const enOut = path.join(websiteRoot, "docs");
const zhOut = path.join(websiteRoot, "i18n/zh-Hans/docusaurus-plugin-content-docs/current");

const rootDocs = [
  { id: "overview", en: "README.md", zh: "README-CN.md", enTitle: "Sophia", zhTitle: "Sophia" },
  { id: "installation", en: "INSTALL.md", zh: "INSTALL-CN.md", enTitle: "Installation and Build", zhTitle: "安装与构建" },
  { id: "contributing", en: "CONTRIBUTING.md", zh: "CONTRIBUTING-CN.md", enTitle: "Contributing Guide", zhTitle: "贡献指南" },
  { id: "changelog", en: "CHANGELOG.md", zh: "CHANGELOG-CN.md", enTitle: "Changelog", zhTitle: "变更日志" },
];

const docPages = [
  "concepts",
  "language_design",
  "language_implementation",
  "engineering_architecture",
  "workflow_graph_spec",
  "type_system",
  "wasm_codegen",
  "stdlib_design",
  "stdlib_implementation",
  "http_lib",
  "file_lib",
  "unit_test",
  "e2e_test",
  "benchmark_test",
];

const rootLinkMap = new Map([
  ["README.md", "overview"],
  ["README-CN.md", "overview"],
  ["INSTALL.md", "installation"],
  ["INSTALL-CN.md", "installation"],
  ["CONTRIBUTING.md", "contributing"],
  ["CONTRIBUTING-CN.md", "contributing"],
  ["CHANGELOG.md", "changelog"],
  ["CHANGELOG-CN.md", "changelog"],
]);

function resetDir(dir) {
  fs.rmSync(dir, { recursive: true, force: true });
  fs.mkdirSync(dir, { recursive: true });
}

function readSource(relativePath) {
  return fs.readFileSync(path.join(repoRoot, relativePath), "utf8");
}

function writeDoc(outDir, id, title, source, sourcePath, position) {
  const body = escapeMdxAngles(rewriteLinks(source, sourcePath)).replace(/^# .+\n/, "");
  const frontMatter = [
    "---",
    `title: ${JSON.stringify(title)}`,
    `sidebar_position: ${position}`,
    "---",
    "",
  ].join("\n");
  fs.writeFileSync(path.join(outDir, `${id}.md`), `${frontMatter}${body}`, "utf8");
}

function titleFromMarkdown(source, fallback) {
  const match = source.match(/^#\s+(.+)$/m);
  return match ? match[1].trim() : fallback;
}

function rewriteLinks(source, sourcePath) {
  return source.replace(/\]\(([^)]+)\)/g, (match, rawTarget) => {
    if (
      rawTarget.startsWith("http://") ||
      rawTarget.startsWith("https://") ||
      rawTarget.startsWith("#")
    ) {
      return match;
    }

    const [target, hash = ""] = rawTarget.split("#");
    const base = path.basename(target);
    if (rootLinkMap.has(base)) {
      return `](./${rootLinkMap.get(base)}${hash ? `#${hash}` : ""})`;
    }

    const docMatch = target.match(/^docs\/(?:en|cn)\/(.+)\.md$/);
    if (docMatch) {
      const docId = docMatch[1].replaceAll("/", "-");
      return `](./${docId}${hash ? `#${hash}` : ""})`;
    }

    if (target === "LICENSE") {
      return "](https://github.com/rongzhou/sophia/blob/main/LICENSE)";
    }

    return match;
  });
}

function escapeMdxAngles(source) {
  return source.replace(/<([A-Za-z][A-Za-z0-9_.,: /-]*)>/g, "&lt;$1&gt;");
}

function syncLocale(outDir, locale) {
  let position = 1;
  for (const doc of rootDocs) {
    const sourcePath = locale === "en" ? doc.en : doc.zh;
    writeDoc(outDir, doc.id, locale === "en" ? doc.enTitle : doc.zhTitle, readSource(sourcePath), sourcePath, position++);
  }

  for (const id of docPages) {
    const sourcePath = `docs/${locale === "en" ? "en" : "cn"}/${id}.md`;
    const source = readSource(sourcePath);
    writeDoc(outDir, id, titleFromMarkdown(source, id), source, sourcePath, position++);
  }
}

resetDir(enOut);
resetDir(zhOut);
syncLocale(enOut, "en");
syncLocale(zhOut, "zh");

console.log(`Synced ${rootDocs.length + docPages.length} docs per locale.`);
