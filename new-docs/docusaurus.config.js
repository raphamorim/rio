// @ts-check
// Note: type annotations allow type checking and IDEs autocompletion

const lightCodeTheme = require('prism-react-renderer/themes/github');
const darkCodeTheme = require('prism-react-renderer/themes/dracula');

/** @type {import('@docusaurus/types').Config} */
const config = {
  title: 'Meet Rio | Rio Terminal',
  tagline: 'A modern terminal for the 21th century.',
  favicon: './static/img/logo.ico',

  // Set the production url of your site here
  url: 'https://raphamorim.io/',
  // Set the /<baseUrl>/ pathname under which your site is served
  // For GitHub pages deployment, it is often '/<projectName>/'
  baseUrl: '/rio',

  // GitHub pages deployment config.
  // If you aren't using GitHub pages, you don't need these.
  organizationName: 'raphamorim', // Usually your GitHub org/user name.
  projectName: 'rio', // Usually your repo name.

  onBrokenLinks: 'throw',
  onBrokenMarkdownLinks: 'warn',

  // Even if you don't use internalization, you can use this field to set useful
  // metadata like html lang. For example, if your site is Chinese, you may want
  // to replace "en" with "zh-Hans".
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
          editUrl:
            'https://github.com/facebook/docusaurus/tree/main/packages/create-docusaurus/templates/shared/',
        },
        blog: {
          showReadingTime: true,
          // Please change this to your repo.
          // Remove this to remove the "edit this page" links.
          editUrl:
            'https://github.com/facebook/docusaurus/tree/main/packages/create-docusaurus/templates/shared/',
        },
        theme: {
          customCss: require.resolve('./src/css/styles.css'),
        },
      }),
    ],
  ],

  themeConfig:
    /** @type {import('@docusaurus/preset-classic').ThemeConfig} */
    ({
      // Replace with your project's social card
      image: 'static/img/logo.ico',
      navbar: {
        title: 'Rio',
        logo: {
          src: 'static/img/logo.ico',
        },
        items: [
		  {to: '/docs/install', label: 'Install', position: 'left'},
		  {to: '/docs/', label: 'Docs', position: 'left'},
		  {to: '/docs/features', label: 'Features', position: 'left'},
          {to: '/blog', label: 'Blog', position: 'left'},
          {
            href: 'https://github.com/raphamorim/rio',
            label: 'GitHub',
            position: 'right',
            image: '/static/img/github-mark-white.png',
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
                to: '/docs/',
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
        copyright: `Copyright © ${new Date().getFullYear()} Rio Built with Docusaurus.`,
      },
      prism: {
        theme: lightCodeTheme,
        darkTheme: darkCodeTheme,
      },
    }),
};

module.exports = config;
