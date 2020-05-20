module.exports = {
  title: 'Saphir Framework',
  tagline: 'Fast, Correct & Easy to Use Http Framework for rust',
  url: 'https://your-docusaurus-test-site.com',
  baseUrl: '/',
  favicon: 'img/favicon.ico',
  organizationName: 'richerarc',
  projectName: 'saphir',
  themeConfig: {
    algolia: {
      apiKey: 'api-key',
      indexName: 'index-name',
      appId: 'app-id', // Optional, if you run the DocSearch crawler on your own
      algoliaOptions: {}, // Optional, if provided by Algolia
    },
    prism: {
      theme: require('prism-react-renderer/themes/dracula'),
      additionalLanguages: ['rust', 'toml'],
    },
    navbar: {
      title: 'Saphir Framework',
      logo: {
        alt: 'Saphir Logo',
        src: 'img/logo.svg',
      },
      links: [
        {
          to: 'docs/start',
          activeBasePath: 'docs',
          label: 'Get Started',
          position: 'left',
        },
        {
          to: 'docs/doc1',
          activeBasePath: 'docs',
          label: 'Documentation',
          position: 'left',
        },
        {
          href: 'https://github.com/richerarc/saphir',
          label: 'Code',
          position: 'right',
        },
        {
          href: 'https://docs.rs/saphir/',
          label: 'Api Doc',
          position: 'right',
        },
      ],
    },
    footer: {
      style: 'dark',
      copyright: `Copyright © ${new Date().getFullYear()} Richer Archambault, Built with Docusaurus.`,
    },
  },
  presets: [
    [
      '@docusaurus/preset-classic',
      {
        docs: {
          sidebarPath: require.resolve('./sidebars.js'),
          // editUrl:
          //   'https://github.com/richerarc/saphir/issues',
        },
        theme: {
          customCss: require.resolve('./src/css/custom.css'),
        },
      },
    ],
  ],
};
