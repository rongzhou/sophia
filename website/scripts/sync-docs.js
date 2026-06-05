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
  "dev_checklist_v1",
  "dev_checklist_v2",
  "language_design",
  "language_implementation",
  "engineering_architecture",
  "workflow_graph_spec",
  "type_system",
  "wasm_codegen",
  "json_lib_design",
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

const docLinkMap = new Map(rootLinkMap);
for (const id of docPages) {
  docLinkMap.set(`${id}.md`, id);
  docLinkMap.set(`docs/${id}.md`, id);
  docLinkMap.set(`docs/en/${id}.md`, id);
  docLinkMap.set(`docs/cn/${id}.md`, id);
}

function resetDir(dir) {
  fs.rmSync(dir, { recursive: true, force: true });
  fs.mkdirSync(dir, { recursive: true });
}

function copyDirIfExists(from, to) {
  if (!fs.existsSync(from)) {
    return;
  }

  fs.cpSync(from, to, { recursive: true });
}

function readSource(relativePath) {
  return fs.readFileSync(path.join(repoRoot, relativePath), "utf8");
}

function writeDoc(outDir, id, title, source, sourcePath, position) {
  const body = prepareMdx(source, sourcePath).replace(/^# .+\n/, "");
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

function prepareMdx(source, sourcePath) {
  return escapeMdxAngles(linkifyBareDocRefs(rewriteLinks(source, sourcePath)));
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
    const normalizedTarget = target.replace(/^\.\//, "");
    const base = path.basename(normalizedTarget);
    if (rootLinkMap.has(base)) {
      return `](./${rootLinkMap.get(base)}${hash ? `#${hash}` : ""})`;
    }

    const docMatch = normalizedTarget.match(/^docs\/(?:en|cn)\/(.+)\.md$/);
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

function linkifyBareDocRefs(source) {
  return transformNonFenceLines(source, linkifyBareDocRefsInLine);
}

function linkifyBareDocRefsInLine(line) {
  const withBacktickLinks = line.replace(/`([^`\n]+\.md)`/g, (match, target) => {
    const docId = docIdForBareTarget(target);
    return docId ? `[${target}](./${docId})` : match;
  });

  return withBacktickLinks.replace(
    /(^|[^\w/.\[\](`-])((?:docs\/(?:en|cn)\/|docs\/)?[A-Za-z0-9_-]+\.md|(?:README|INSTALL|CONTRIBUTING|CHANGELOG)(?:-CN)?\.md)\b/g,
    (match, prefix, target) => {
      const docId = docIdForBareTarget(target);
      return docId ? `${prefix}[${target}](./${docId})` : match;
    },
  );
}

function docIdForBareTarget(target) {
  const normalizedTarget = target.replace(/^\.\//, "");
  return docLinkMap.get(normalizedTarget) || docLinkMap.get(path.basename(normalizedTarget));
}

function escapeMdxAngles(source) {
  return transformNonFenceChunks(source, escapeMdxAnglesInChunk);
}

function escapeMdxAnglesInChunk(chunk) {
  let result = "";
  let cursor = 0;
  const codeSpan = /(`+)([\s\S]*?)\1/g;
  let match;

  while ((match = codeSpan.exec(chunk)) !== null) {
    result += escapeMdxAnglesInText(chunk.slice(cursor, match.index));
    result += match[0];
    cursor = match.index + match[0].length;
  }

  return result + escapeMdxAnglesInText(chunk.slice(cursor));
}

function escapeMdxAnglesInText(text) {
  return text.replace(/<([A-Za-z][A-Za-z0-9_.,: /-]*)>/g, "&lt;$1&gt;");
}

function transformNonFenceLines(source, transformLine) {
  const lines = source.split("\n");
  let inFence = false;

  return lines
    .map((line) => {
      if (/^\s*(```|~~~)/.test(line)) {
        inFence = !inFence;
        return line;
      }

      return inFence ? line : transformLine(line);
    })
    .join("\n");
}

function transformNonFenceChunks(source, transformChunk) {
  const lines = source.split("\n");
  const output = [];
  let chunk = [];
  let inFence = false;

  function flushChunk() {
    if (chunk.length > 0) {
      output.push(transformChunk(chunk.join("\n")));
      chunk = [];
    }
  }

  for (const line of lines) {
    if (/^\s*(```|~~~)/.test(line)) {
      flushChunk();
      output.push(line);
      inFence = !inFence;
    } else if (inFence) {
      output.push(line);
    } else {
      chunk.push(line);
    }
  }

  flushChunk();
  return output.join("\n");
}

function syncLocale(outDir, locale) {
  copyDirIfExists(path.join(repoRoot, `docs/${locale === "en" ? "en" : "cn"}/images`), path.join(outDir, "images"));

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
