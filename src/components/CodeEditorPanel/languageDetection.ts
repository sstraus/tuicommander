import { LanguageDescription, type LanguageSupport } from "@codemirror/language";
import { languages } from "@codemirror/language-data";

/** Files without a recognized extension that should use Shell highlighting */
const SHELL_FILENAMES = /^(Makefile|GNUmakefile|makefile)$/;

/** Detect the CodeMirror language support for a filename using language-data's 143-language registry */
export async function detectLanguage(filename: string): Promise<LanguageSupport | null> {
  const basename = filename.split("/").pop() ?? filename;

  const desc = LanguageDescription.matchFilename(languages, basename);
  if (desc) return desc.load();

  // Makefile and friends aren't in language-data — use Shell as approximation
  if (SHELL_FILENAMES.test(basename)) {
    const shell = LanguageDescription.matchLanguageName(languages, "Shell");
    if (shell) return shell.load();
  }

  return null;
}
