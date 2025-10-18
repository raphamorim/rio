// @ts-check
// Note: type annotations allow type checking and IDEs autocompletion

const { themes } = require('prism-react-renderer');
const lightCodeTheme = themes.github;
const darkCodeTheme = themes.dracula;

const defaultLocale = 'en';
const CURRENT_LOCALE = process.env.DOCUSAURUS_CURRENT_LOCALE ?? defaultLocale;

const tagline = {
  en: 'A modern terminal for the 21st century.',
  ko: '21세기의 현대적인 터미널.',
  'pt-br': 'Terminal moderno para o século 21',
  es: 'Una terminal moderna para el siglo 21.',
  pl: 'Nowoczesny terminal na miarę XXI wieku.',
  ja: '21世紀のモダンターミナル',
  'zh-hans': '21 世纪的现代终端',
  'zh-hant': '21 世紀的現代終端',
};

/** @type {import('@docusaurus/types').Config} */
const config = {
  title: 'Rio Terminal',
  tagline: tagline[CURRENT_LOCALE],
  favicon: '/assets/rio-logo.ico',
  url: 'https://rioterm.com',
  trailingSlash: false,
  baseUrl: '/',
  organizationName: 'raphamorim',
  projectName: 'rio',
  onBrokenLinks: 'throw',
  onBrokenMarkdownLinks: 'warn',
  i18n: {
    defaultLocale,
    locales: ['en', 'ko', 'pt-br', 'es', 'pl', 'ja', 'zh-hans', 'zh-hant'],
  },

  headTags: [
    {
      tagName: 'link',
      attributes: {
        rel: 'preconnect',
        href: 'https://fonts.googleapis.com',
      },
    },
    {
      tagName: 'link',
      attributes: {
        rel: 'preconnect',
        href: 'https://fonts.gstatic.com',
        crossorigin: 'anonymous',
      },
    },
    {
      tagName: 'link',
      attributes: {
        rel: 'stylesheet',
        href: 'https://fonts.googleapis.com/css2?family=Noto+Sans:ital,wght@0,400;0,500;0,700;1,400;1,500;1,700&display=swap',
      },
    },
  ],

  presets: [
    [
      'classic',
      /** @type {import('@docusaurus/preset-classic').Options} */
      ({
        docs: {
          sidebarPath: require.resolve('./sidebars.js'),
          editUrl: 'https://github.com/raphamorim/rio/tree/main/docs/',
          disableVersioning: false,
        },
        blog: {
          showReadingTime: true,
          // Please change this to your repo.
          // Remove this to remove the "edit this page" links.
          editUrl: 'https://github.com/raphamorim/rio/tree/main/docs/',
        },
        theme: {
          customCss: [
            require.resolve('react-tweet/theme.css'),
            require.resolve('./src/css/custom.css'),
          ],
        },
        gtag: {
          trackingID: 'G-6MKJ1X7CFS',
          anonymizeIP: true,
        },
      }),
    ],
  ],

  themeConfig:
    /** @type {import('@docusaurus/preset-classic').ThemeConfig} */
    ({
      // Replace with your project's social card
      image: '/assets/banner.png',
      navbar: {
        logo: {
          src: '/assets/rio-logo.svg',
        },
        items: [
          { to: '/docs/install', label: 'Install', position: 'left' },
          { to: '/docs/config', label: 'Config', position: 'left', },
          { to: '/docs/features', label: 'Features', position: 'left' },
          { to: '/changelog', label: 'Changelog', position: 'left' },
          { to: '/blog', label: 'Blog', position: 'left' },
          {
            href: 'https://discord.gg/zRvJjmKGwS',
            label: 'Discord',
            position: 'left',
          },
          {
            type: 'localeDropdown',
            position: 'right',
          },
          {
            href: 'https://github.com/raphamorim/rio',
            label: 'GitHub',
            position: 'right',
            // image: '/assets/github-mark.svg',
          },
        ],
      },
      footer: {
        style: 'dark',
        links: [
          {
            title: 'Docs',
            items: [
              {
                label: 'Install',
                to: '/docs/install',
              },
              {
                label: 'Config',
                to: '/docs/config',
              },
              {
                label: 'Features',
                to: '/docs/features',
              },
            ],
          },
          {
            title: 'Community',
            items: [
              {
                label: 'Discord',
                href: 'https://discord.gg/zRvJjmKGwS',
              },
              {
                label: 'Twitter',
                href: 'https://twitter.com/raphamorims',
              },
            ],
          },
          {
            title: 'More',
            items: [
              {
                label: 'Blog',
                to: '/blog',
              },
              {
                label: 'GitHub',
                href: 'https://github.com/raphamorim/rio',
              },
            ],
          },
        ],
        copyright: `Copyright © 2023-${new Date().getFullYear()} Rio Terminal.`,
      },
      prism: {
        theme: lightCodeTheme,
        darkTheme: darkCodeTheme,
        additionalLanguages: ['bash', 'toml', 'nix'],
      },

      colorMode: {
        defaultMode: 'dark',
        disableSwitch: false,
        respectPrefersColorScheme: false,
      },

      announcementBar: {
        id: 'support_us',
        content:
          'Support Rio Terminal via <a target="_blank" rel="noopener noreferrer" href="https://github.com/sponsors/raphamorim">GitHub Sponsors</a>',
        backgroundColor: '#f712ff',
        textColor: '#FFFFFF',
        isCloseable: true,
      },

      algolia: {
        // The application ID provided by Algolia
        appId: '6KTBGQQMEX',
        // Public API key: it is safe to commit it
        apiKey: 'debd45deb1f0785248bdde28ec768d5a',
        indexName: 'raphamorim',
        debug: false,
      },
    }),
};

module.exports = config;
