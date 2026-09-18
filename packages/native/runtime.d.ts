import type { GpuixRenderer } from "./index";

export interface NativeRuntimeInfo {
  apiVersion: 1;
  extensionApiVersion: 1;
  version: string;
  extensions: Array<{
    id: string;
    version: string;
    apiVersion: number;
    elements: string[];
  }>;
}

/** No native module is loaded by importing this entry. */
export function configureNativeBindings(bindings: {
  GpuixRenderer: typeof GpuixRenderer;
  nativeRuntimeInfo(): string;
  [name: string]: unknown;
}): NativeRuntimeInfo;

export function getNativeBindings(): Record<string, unknown> | undefined;
