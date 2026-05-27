// @ts-check

const config = {
  title: "Sophia",
  tagline: "An LLM-native graph programming path beyond code pretraining",
  url: "https://rongzhou.github.io",
  baseUrl: "/sophia/",
  organizationName: "rongzhou",
  projectName: "sophia",
  trailingSlash: false,

  i18n: {
    defaultLocale: "en",
    locales: ["en", "zh-Hans"],
    localeConfigs: {
      en: {
        label: "English",
      },
      "zh-Hans": {
        label: "简体中文",
      },
    },
  },

  onBrokenLinks: "throw",
  markdown: {
    hooks: {
      onBrokenMarkdownLinks: "warn",
    },
  },

  presets: [
    [
      "classic",
      {
        docs: {
          sidebarPath: "./sidebars.js",
          routeBasePath: "docs",
          editUrl: "https://github.com/rongzhou/sophia/tree/main/website/",
        },
        blog: false,
        theme: {
          customCss: "./src/css/custom.css",
        },
      },
    ],
  ],

  themeConfig: {
    navbar: {
      title: "Sophia",
      items: [
        {
          type: "docSidebar",
          sidebarId: "docsSidebar",
          position: "left",
          label: "Documents",
        },
        {
          type: "localeDropdown",
          position: "right",
        },
        {
          href: "https://github.com/rongzhou/sophia",
          label: "GitHub",
          position: "right",
        },
      ],
    },
    footer: {
      style: "dark",
      links: [
        {
          title: "Documents",
          items: [
            { label: "Language Design", to: "/docs/language-design" },
            { label: "Technical Report v0.2", to: "/docs/technical-report-v0-2" },
            { label: "Heuristic Workflow", to: "/docs/heuristic-workflow" },
          ],
        },
        {
          title: "Project",
          items: [{ label: "GitHub", href: "https://github.com/rongzhou/sophia" }],
        },
      ],
      copyright: `Copyright © ${new Date().getFullYear()} Sophia.`,
    },
    prism: {
      additionalLanguages: ["bash", "json", "typescript"],
    },
  },
};

module.exports = config;
