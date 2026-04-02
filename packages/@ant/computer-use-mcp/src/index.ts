/**
* @ant/computer-use-mcp — Stub implementation
 *
 * Provide a type-safe stub with all functions returning sensible default values.
 * Will not be actually called when feature('CHICAGO_MCP') = false,
 * But make sure the import does not report an error and the type is correct.
 */

import type {
  ComputerUseHostAdapter,
  CoordinateMode,
  GrantFlags,
  Logger,
} from './types'

// Re-export types from types.ts
export type { CoordinateMode, Logger } from './types'
export type {
  ComputerUseConfig,
  ComputerUseHostAdapter,
  CuPermissionRequest,
  CuPermissionResponse,
  CuSubGates,
} from './types'
export { DEFAULT_GRANT_FLAGS } from './types'

// ---------------------------------------------------------------------------
// Types (defined here for callers that import from the main entry)
// ---------------------------------------------------------------------------

export interface DisplayGeometry {
  width: number
  height: number
  displayId?: number
  originX?: number
  originY?: number
}

export interface FrontmostApp {
  bundleId: string
  displayName: string
}

export interface InstalledApp {
  bundleId: string
  displayName: string
  path: string
}

export interface RunningApp {
  bundleId: string
  displayName: string
}

export interface ScreenshotResult {
  base64: string
  width: number
  height: number
}

export type ResolvePrepareCaptureResult = ScreenshotResult

export interface ScreenshotDims {
  width: number
  height: number
  displayWidth: number
  displayHeight: number
  displayId: number
  originX: number
  originY: number
}

export interface CuCallToolResultContent {
  type: 'image' | 'text'
  data?: string
  mimeType?: string
  text?: string
}

export interface CuCallToolResult {
  content: CuCallToolResultContent[]
  telemetry: {
    error_kind?: string
    [key: string]: unknown
  }
}

export type ComputerUseSessionContext = Record<string, unknown>

// ---------------------------------------------------------------------------
// API_RESIZE_PARAMS — Default screenshot scaling parameters
// ---------------------------------------------------------------------------

export const API_RESIZE_PARAMS = {
  maxWidth: 1280,
  maxHeight: 800,
  maxPixels: 1280 * 800,
}

// ---------------------------------------------------------------------------
// ComputerExecutor — stub class
// ---------------------------------------------------------------------------

export class ComputerExecutor {
  capabilities: Record<string, boolean> = {}
}

// ---------------------------------------------------------------------------
// Functions — stubs that return sensible default values
// ---------------------------------------------------------------------------

/**
 * Calculate target screenshot size.
 * Choose the optimal size between physical width and height and API limits.
 */
export function targetImageSize(
  physW: number,
  physH: number,
  _params?: typeof API_RESIZE_PARAMS,
): [number, number] {
  const maxW = _params?.maxWidth ?? 1280
  const maxH = _params?.maxHeight ?? 800
  const scale = Math.min(1, maxW / physW, maxH / physH)
  return [Math.round(physW * scale), Math.round(physH * scale)]
}

/**
 * Bind the session context and return the tool scheduling function.
 * Stub returns a scheduler that always returns empty results.
 */
export function bindSessionContext(
  _adapter: ComputerUseHostAdapter,
  _coordinateMode: CoordinateMode,
  _ctx: ComputerUseSessionContext,
): (name: string, args: unknown) => Promise<CuCallToolResult> {
  return async (_name: string, _args: unknown) => ({
    content: [],
    telemetry: {},
  })
}

/**
 * Build a list of Computer Use tool definitions.
 * Stub returns an empty array (no tools).
 */
export function buildComputerUseTools(
  _capabilities?: Record<string, boolean>,
  _coordinateMode?: CoordinateMode,
  _installedAppNames?: string[],
): Array<{ name: string; description: string; inputSchema: Record<string, unknown> }> {
  return []
}

/**
 * Create Computer Use MCP server.
 * Stub returns null (service is not enabled).
 */
export function createComputerUseMcpServer(
  _adapter?: ComputerUseHostAdapter,
  _coordinateMode?: CoordinateMode,
): null {
  return null
}
