/**
 * Shape Handle - Event subscriptions for synced data changes
 *
 * Simple EventEmitter-like class for all structure types.
 * For reading data, use your Redis client directly.
 */

import type { Unsubscribe } from './types'

/**
 * Shape handle - provides event subscriptions for any structure type
 *
 * @example
 * ```typescript
 * const handle = sync.syncShape<User>('users', { shape: { table: 'users' } })
 *
 * handle.onInsert((key, data) => console.log('Inserted:', key, data))
 * handle.onUpdate((key, data) => console.log('Updated:', key, data))
 * handle.onDelete((key) => console.log('Deleted:', key))
 * ```
 */
export class ShapeHandle<T = unknown> {
  readonly name: string
  readonly redisKey: string

  #insertCallbacks = new Set<(key: string, data: T) => void>()
  #updateCallbacks = new Set<(key: string, data: T) => void>()
  #deleteCallbacks = new Set<(key: string) => void>()

  constructor(name: string, redisKey: string) {
    this.name = name
    this.redisKey = redisKey
  }

  /**
   * Subscribe to insert events
   */
  onInsert(callback: (key: string, data: T) => void): Unsubscribe {
    this.#insertCallbacks.add(callback)
    return () => this.#insertCallbacks.delete(callback)
  }

  /**
   * Subscribe to update events
   */
  onUpdate(callback: (key: string, data: T) => void): Unsubscribe {
    this.#updateCallbacks.add(callback)
    return () => this.#updateCallbacks.delete(callback)
  }

  /**
   * Subscribe to delete events
   */
  onDelete(callback: (key: string) => void): Unsubscribe {
    this.#deleteCallbacks.add(callback)
    return () => this.#deleteCallbacks.delete(callback)
  }

  /**
   * @internal Trigger callbacks for an operation
   */
  trigger(operation: string, key: string, data: unknown): void {
    switch (operation) {
      case `insert`:
        if (data) this.#insertCallbacks.forEach((cb) => cb(key, data as T))
        break
      case `update`:
        if (data) this.#updateCallbacks.forEach((cb) => cb(key, data as T))
        break
      case `delete`:
        this.#deleteCallbacks.forEach((cb) => cb(key))
        break
    }
  }
}
