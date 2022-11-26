import React from 'react';


interface VimeoProps {
    id: string;
}


export function Vimeo({ id }: VimeoProps)
{
    return (
        <iframe
          src={`https://player.vimeo.com/video/${id}`}
          width="640"
          height="360"
          frameborder="0"
          webkitallowfullscreen mozallowfullscreen allowfullscreen
        >
        </iframe>
    );
}
