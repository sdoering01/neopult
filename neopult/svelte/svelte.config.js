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
        paths: {
            // SETUP: Change this to the path where your site will be hosted,
            // if it differs from the root. For example, if you plan to host
            // your site at https://example.com/blog/, change this to '/blog'
            // (note the leading but not trailing slash).
            base: '',
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
