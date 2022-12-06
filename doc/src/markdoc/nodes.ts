import { CodeBlock, Heading, Link } from '../components';

export const link = {
    render: Link,
    attributes: {
        href: {
            type: String,
        },
    },
};

export const fence = {
    render: CodeBlock,
    attributes: {
        content: {
            type: String,
        },
        language: {
            type: String,
        },
    },
};

export const heading = {
    render: Heading,
    attributes: {
        level: {
            type: Number,
            required: true,
            default: 1,
        },
    },
};
