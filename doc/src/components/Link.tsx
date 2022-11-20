import React from 'react';
import NextjsLink from 'next/link';


interface LinkProps {
    children: any;
    href: string;
}


export function Link({ children, href }: LinkProps)
{
    return (
        <>
            {href.startsWith("https://") || href.startsWith("http://") ? (
                <a href={href}>{children}</a>
            ) : (
                <NextjsLink href={href}><a>{children}</a></NextjsLink>
            )}
        </>
    );
}
