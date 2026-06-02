// @ts-check

const config = {
  title: "Sophia",
  tagline: "Deterministic semantic programming for LLM-native systems",
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
        label: "英文",
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
          editUrl: "https://github.com/rongzhou/sophia/tree/main/",
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
          label: "Docs",
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
          title: "Docs",
          items: [
            { label: "Overview", to: "/docs/overview" },
            { label: "Concepts", to: "/docs/concepts" },
            { label: "Install", to: "/docs/installation" },
          ],
        },
        {
          title: "Project",
          items: [
            { label: "GitHub", href: "https://github.com/rongzhou/sophia" },
            { label: "License", href: "https://github.com/rongzhou/sophia/blob/main/LICENSE" },
          ],
        },
      ],
      copyright: `Copyright © ${new Date().getFullYear()} Sophia.`,
    },
    prism: {
      additionalLanguages: ["bash", "json", "rust", "toml"],
    },
  },
};

module.exports = config;
