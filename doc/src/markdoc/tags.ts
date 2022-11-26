import { Vimeo } from '../components';

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
