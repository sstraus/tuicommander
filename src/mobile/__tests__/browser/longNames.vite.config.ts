import { defineConfig } from "vite";
import solid from "vite-plugin-solid";
export default defineConfig({
	plugins: [
		{
			name: "long-name-transport",
			enforce: "pre",
			resolveId(id) {
				if (/(^|\/)transport$/.test(id)) return "\0long-name-transport";
			},
			load(id) {
				if (id === "\0long-name-transport")
					return `
 export * from "/src/transport.ts";
 export async function rpc(command) {
  const long = 'a'.repeat(60);
  if (command === 'load_repositories') return {repos:{['/layout/'+long]:{}}};
  if (command === 'list_directory') return [{name:long,path:long,is_dir:true,size:0}];
  if (command === 'progress_list') return {project:'/layout',entries:[{id:1,type:'done',text:long,step:long}],nextCursor:null};
  return [];
 }
 `;
			},
		},
		solid(),
		{
			name: "mobile-long-name-fixture",
			transformIndexHtml(html) {
				return html.replace("/src/mobile/index.tsx", "/src/mobile/__tests__/browser/longNames.tsx");
			},
		},
	],
	server: { host: "127.0.0.1", port: 14369, strictPort: true },
});
