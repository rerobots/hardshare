const withMarkdoc = require('@markdoc/next.js');

module.exports = withMarkdoc({
})({
    pageExtensions: ['ts', 'tsx', 'md'],
});
