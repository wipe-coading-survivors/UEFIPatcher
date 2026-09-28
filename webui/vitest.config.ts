import path from 'node:path';
import { defineConfig } from 'vitest/config';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { svelteTesting } from '@testing-library/svelte/vite';

export default defineConfig({
    plugins: [svelteTesting(), svelte({ hot: false })],
    resolve: {
        alias: {
            $lib: path.resolve('./src/lib'),
            '$app/navigation': path.resolve('./src/lib/test/navigation.ts'),
        },
    },
    test: {
        environment: 'jsdom',
        setupFiles: ['./src/lib/test/setup.ts'],
    },
});
