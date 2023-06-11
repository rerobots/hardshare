import React from 'react';


interface VimeoProps {
    id: string;
}


export function Vimeo({ id }: VimeoProps)
{
    return (
        <iframe
          className="vimeoIframe"
          src={`https://player.vimeo.com/video/${id}`}
          width="640"
          height="360"
          frameBorder="0"
          allowFullScreen
        >
        </iframe>
    );
}
