import { Image, Vimeo } from '../components';

export const image = {
    render: Image,
    description: 'Image with shape constraints',
    attributes: {
        src: {
            type: String,
            errorLevel: 'critical',
            required: true,
        },
        alt: {
            type: String,
        },
        maxWidth: {
            type: String,
        },
    },
};


export const vimeo = {
    render: Vimeo,
    description: 'Embeds a video on Vimeo',
    attributes: {
        id: {
            type: String,
            errorLevel: 'critical',
            required: true,
        },
    },
};
