const withMarkdoc = require('@markdoc/next.js');

module.exports = withMarkdoc({
    schemaPath: 'src/markdoc',
})({
    pageExtensions: ['ts', 'tsx', 'md'],
});
