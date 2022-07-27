import adapter from '@sveltejs/adapter-static';
import preprocess from 'svelte-preprocess';

/** @type {import('@sveltejs/kit').Config} */
const config = {
    preprocess: preprocess({ postcss: true }),

    kit: {
        adapter: adapter(),
        alias: {
            $components: 'src/components',
        },
        prerender: {
            default: true,
        },
    },
    files: {
        lib: 'lib',
    },
};

export default config;
