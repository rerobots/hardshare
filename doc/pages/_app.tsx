import React from 'react';

import type { AppProps } from 'next/app';
import type { MarkdocNextJsPageProps } from '@markdoc/next.js';


export default function App({ Component, pageProps }: AppProps<MarkdocNextJsPageProps>)
{
    const { markdoc } = pageProps;

    let title = 'hardshare';
    if (markdoc?.frontmatter?.title) {
        title += ': ' + markdoc.frontmatter.title;
    } else {
        title += '  documentation';
    }

    return (
        <>
            <div id="main-content">
                <Component {...pageProps} />
            </div>
        </>
    );
}
