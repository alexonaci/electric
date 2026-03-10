import type {
  RedisMultiLike,
  RedisStructure,
  InternalShapeConfig,
} from '../types'
import type { ScriptShaCache } from '../lua-scripts'
import { handleHash } from './hash'
import { handleStream } from './streams'
import { handleSet } from './set'
import { handleSortedSet } from './sorted-set'

export type Operation = `insert` | `update` | `delete`

/**
 * Routes operations to the appropriate handler using Lua scripts
 */
export function handleOperation(
  pipeline: RedisMultiLike,
  structure: RedisStructure,
  operation: Operation,
  key: string,
  data: Record<string, unknown> | null,
  shapeName: string,
  scripts: ScriptShaCache,
  config?: InternalShapeConfig
): void {
  switch (structure) {
    case `hash`:
      return handleHash(pipeline, operation, key, data, shapeName, scripts)
    case `stream`:
      return handleStream(pipeline, operation, key, data, shapeName, scripts)
    case `set`:
      return handleSet(pipeline, operation, key, shapeName, scripts)
    case `sorted_set`:
      return handleSortedSet(
        pipeline,
        operation,
        key,
        data,
        shapeName,
        scripts,
        config
      )
    default:
      throw new Error(`Unsupported Redis structure: ${structure}`)
  }
}

// Export individual handlers for advanced usage
export { handleHash } from './hash'
export { handleStream } from './streams'
export { handleSet } from './set'
export { handleSortedSet } from './sorted-set'
