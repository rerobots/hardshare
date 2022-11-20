import { CodeBlock } from '../components';
import { Link } from '../components';

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
