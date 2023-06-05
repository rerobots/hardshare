import React from 'react';


interface ImageProps {
    src: string;
    alt: string;
    maxWidth: string;
}

export function Image({ src, alt, maxWidth }: ImageProps)
{
    let style = undefined;
    if (maxWidth) {
        style = {
            maxWidth,
        };
    }

    return (
        <img src={src} alt={alt} style={style} />
    );
}
