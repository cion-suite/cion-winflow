import boundaries from "eslint-plugin-boundaries";
import reactHooks from "eslint-plugin-react-hooks";
import { defineConfig, globalIgnores } from "eslint/config";
import tseslint from "typescript-eslint";

const LOWER = { element: { types: ["ui", "lib"] } };
const API_ENTRY = {
  element: { type: "api", fileInternalPath: "{index.ts,client.ts}" },
};

export default defineConfig([
  globalIgnores(["dist", "src-tauri", "src/ui/shadcn"]),
  ...tseslint.configs.recommended,
  {
    files: ["src/**/*.{ts,tsx}", "vite.config.ts"],
    languageOptions: {
      parserOptions: {
        projectService: true,
        tsconfigRootDir: import.meta.dirname,
      },
    },
    rules: {
      "@typescript-eslint/no-floating-promises": "error",
    },
  },
  {
    files: ["src/**/*.{ts,tsx}"],
    plugins: { boundaries, "react-hooks": reactHooks },
    settings: {
      "boundaries/root-path": import.meta.dirname,
      "boundaries/include": ["src/**/*.{ts,tsx}"],
      "boundaries/elements": [
        { type: "app", pattern: "src/app", partialMatch: false },
        { type: "shell", pattern: "src/shell", partialMatch: false },
        { type: "pages", pattern: "src/pages/*", partialMatch: false },
        { type: "api", pattern: "src/api", partialMatch: false },
        { type: "ui", pattern: "src/ui", partialMatch: false },
        { type: "lib", pattern: "src/lib", partialMatch: false },
      ],
      "boundaries/additional-dependency-nodes": [
        { selector: "TSImportType > Literal", kind: "type", name: "import-type" },
      ],
      "import/resolver": {
        typescript: { project: `${import.meta.dirname}/tsconfig.json` },
      },
    },
    rules: {
      ...reactHooks.configs.recommended.rules,
      "boundaries/no-unknown-files": "error",
      "boundaries/dependencies": [
        "error",
        {
          default: "disallow",
          policies: [
            {
              from: { element: { type: "app" } },
              allow: {
                to: [
                  { element: { types: ["shell", "pages"] } },
                  API_ENTRY,
                  LOWER,
                ],
              },
            },
            {
              from: { element: { types: ["shell", "pages"] } },
              allow: { to: [API_ENTRY, LOWER] },
            },
            {
              from: { element: { types: ["ui", "api"] } },
              allow: { to: { element: { type: "lib" } } },
            },
          ],
        },
      ],
    },
  },
  {
    files: ["src/**/*.{ts,tsx}"],
    ignores: ["src/api/client.ts"],
    rules: {
      "no-restricted-imports": [
        "error",
        {
          paths: [
            {
              name: "@tauri-apps/api/core",
              importNames: ["invoke"],
              message: "Rust is called through src/api, invoke lives in src/api/client.ts.",
            },
            {
              name: "@tauri-apps/api",
              message: "Import from a subpath such as @tauri-apps/api/event: the root exposes invoke.",
            },
          ],
        },
      ],
    },
  },
]);
