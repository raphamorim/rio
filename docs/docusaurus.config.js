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
};

/** @type {import('@docusaurus/types').Config} */
const config = {
  title: 'Rio Terminal',
  tagline: tagline[CURRENT_LOCALE],
  favicon: '/assets/rio-logo.ico',
  url: 'https://raphamorim.io',
  baseUrl: '/rio',
  organizationName: 'raphamorim',
  projectName: 'rio',
  onBrokenLinks: 'throw',
  onBrokenMarkdownLinks: 'warn',
  i18n: {
    defaultLocale,
    locales: ['en', 'ko', 'pt-br', 'es', 'pl'],
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
          // Please change this to your repo.
          // Remove this to remove the "edit this page" links.
          editUrl: 'https://github.com/raphamorim/rio/tree/main/docs/',
          disableVersioning: false,
          includeCurrentVersion: true,
          lastVersion: undefined,
          onlyIncludeVersions: ['current', '0.0.x'],
          versions: {
            current: {
              label: '1.0.0 (unreleased)',
              path: 'next',
              banner: 'none',
            },
            '0.0.x': {
              label: '0.0.x',
              path: '0.0.x',
              banner: 'none',
            },
          },
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
        // gtag: {
        //   trackingID: '---------',
        //   anonymizeIP: true,
        // },
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
          { to: '/docs/next/install', label: 'Install', position: 'left' },
          {
            to: '/docs/next/configuration-file',
            label: 'Config',
            position: 'left',
          },
          { to: '/docs/next/features', label: 'Features', position: 'left' },
          { to: '/blog', label: 'Blog', position: 'left' },
          {
            href: 'https://discord.gg/zRvJjmKGwS',
            label: 'Discord',
            position: 'left',
          },
          {
            type: 'docsVersionDropdown',
            position: 'right',
            // dropdownItemsAfter: [{to: '/versions', label: 'All versions'}],
            dropdownActiveClassDisabled: true,
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
                to: '/docs/next/install',
              },
              {
                label: 'Config',
                to: '/docs/next/configuration-file',
              },
              {
                label: 'Features',
                to: '/docs/next/features',
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
        copyright: `Copyright © ${new Date().getFullYear()} Rio Terminal.`,
      },
      prism: {
        theme: lightCodeTheme,
        darkTheme: darkCodeTheme,
        additionalLanguages: ['bash', 'toml'],
      },

      colorMode: {
        defaultMode: 'dark',
        disableSwitch: false,
        respectPrefersColorScheme: false,
      },

      announcementBar: {
        id: 'support_us',
        content:
          'Support Rio via <a target="_blank" rel="noopener noreferrer" href="https://github.com/sponsors/raphamorim">github sponsors</a>',
        backgroundColor: '#f712ff',
        textColor: '#FFFFFF',
        isCloseable: true,
      },
    }),
};

module.exports = config;
