import React from 'react';


interface HeadingProps {
    children: string;
    level: number;
}

export function Heading({ children, level }: HeadingProps)
{
    const anchor = children.toLowerCase().split(/ +/).join('-')
    return React.createElement(
        `h${level}`,
        {'id': anchor},
        children
    );
}
