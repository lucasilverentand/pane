// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

// https://astro.build/config
export default defineConfig({
	integrations: [
		starlight({
			title: 'pane',
			description: 'A terminal multiplexer for modern development workflows.',
			logo: {
				light: './src/assets/logo.svg',
				dark: './src/assets/logo.svg',
				replacesTitle: false,
			},
			social: [{ icon: 'github', label: 'GitHub', href: 'https://github.com/lucasilverentand/pane' }],
			customCss: ['./src/styles/custom.css'],
			sidebar: [
				{
					label: 'Getting Started',
					items: [
						{ label: 'Install and Usage', slug: 'getting-started/install' },
					],
				},
				{
					label: 'Reference',
					items: [
						{ label: 'Configuration', slug: 'reference/configuration' },
						{ label: 'Architecture', slug: 'reference/architecture' },
					],
				},
				{
					label: 'Project',
					items: [{ label: 'Development', slug: 'project/development' }],
				},
			],
			editLink: {
				baseUrl: 'https://github.com/lucasilverentand/pane/edit/main/site/',
			},
		}),
	],
});
