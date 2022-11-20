import React, { useEffect, useRef } from 'react';
import Prism from 'prismjs';


interface CodeBlockProps {
    children: any;
    language: string;
}

export function CodeBlock({ children, language }: CodeBlockProps)
{
    const ref = useRef(null);

    useEffect(() => {
        if (ref.current) {
            Prism.highlightElement(ref.current, false);
        }
    }, [children]);

    return (
        <pre><code ref={ref} className={`language-${language}`}>
            {children}
        </code></pre>
    );
}
