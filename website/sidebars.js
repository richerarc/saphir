module.exports = {
  someSidebar: {
    Quickstart: ['start', 'start2'],
    Documentation: [
      'stack', 
      'middleware', 
      'controller',
      'handlers',
      {
        type: 'category',
        label: 'Http Types',
        items: ['request', 'response'],
      },
      {
        type: 'category',
        label: 'Advanced Features',
        items: [],
      },
    ],
  },
};
