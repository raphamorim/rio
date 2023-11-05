// @ts-check
// Note: type annotations allow type checking and IDEs autocompletion

const { themes } = require('prism-react-renderer');
const lightCodeTheme = themes.github;
const darkCodeTheme = themes.dracula;

/** @type {import('@docusaurus/types').Config} */
const config = {
  title: 'Rio Terminal',
  tagline: 'A modern terminal for the 21st century.',
  favicon: '/assets/rio-logo.ico',
  url: 'https://raphamorim.io',
  baseUrl: '/rio',
  organizationName: 'raphamorim',
  projectName: 'rio',
  onBrokenLinks: 'throw',
  onBrokenMarkdownLinks: 'warn',
  i18n: {
    defaultLocale: 'en',
    locales: ['en'],
  },

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
          { to: '/docs/documentation', label: 'Docs', position: 'left' },
          { to: '/docs/features', label: 'Features', position: 'left' },
          { to: '/blog', label: 'Blog', position: 'left' },
          {
            href: 'https://github.com/raphamorim/rio',
            label: 'GitHub',
            position: 'right',
            // image: '/assets/github-mark.svg',
          },
          {
            href: 'https://discord.gg/zRvJjmKGwS',
            label: 'Discord',
            position: 'left',
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
                label: 'Docs',
                to: '/docs/documentation',
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
          'If you use Rio terminal please consider support via <a target="_blank" rel="noopener noreferrer" href="https://github.com/sponsors/raphamorim">github sponsors</a>',
        backgroundColor: '#f712ff',
        textColor: '#FFFFFF',
        isCloseable: true,
      },
    }),
};

module.exports = config;
