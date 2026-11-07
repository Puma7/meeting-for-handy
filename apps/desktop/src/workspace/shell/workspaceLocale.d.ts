import type { Context } from "react";
export const WorkspaceLanguage: Context<"de" | "en">;
export function workspaceText(language: string, text: string): string;
export function useWorkspaceText(): (text: string) => string;
