import {
  ShapeStream,
  Message,
  isChangeMessage,
  type ShapeStreamOptions,
} from '@electric-sql/client'
import type {
  ElectricRedisConfig,
  ShapeConfig,
  HashShapeConfig,
  SortedSetShapeConfig,
  InternalShapeConfig,
  RedisClientLike,
} from './types'
import { chunk } from './utils'
import { loadAllScripts, type ScriptShaCache } from './lua-scripts'
import { handleOperation } from './handlers'
import { ShapeHandle } from './shape-handles'

// Internal config with the full RedisClientLike type for internal use
interface InternalConfig
  extends Omit<ElectricRedisConfig, `redis` | `batchSize`> {
  redis: RedisClientLike
  batchSize: number
}

/**
 * ElectricRedis - Redis integration for Electric SQL
 *
 * Syncs Electric SQL Shapes to Redis data structures.
 * Supports: hash, set, sorted_set, stream
 *
 * Compatible with: node-redis, ioredis, iovalkey
 *
 * @example
 * ```typescript
 * import { createClient } from 'redis'
 * import { ElectricRedis } from '@electric-sql/redis'
 *
 * const redis = createClient({ url: 'redis://localhost:6379' })
 *
 * const sync = new ElectricRedis({
 *   electric: { url: 'http://localhost:3000/v1/shape' },
 *   redis,
 * })
 *
 * const users = sync.syncShape<User>('users', {
 *   shape: { table: 'users' },
 *   structure: 'hash'
 * })
 *
 * users.onInsert((key, user) => console.log('New user:', user.name))
 *
 * await sync.start()
 *
 * // Read data using your Redis client:
 * const allUsers = await redis.hGetAll('users')
 * ```
 */
export class ElectricRedis {
  #config: InternalConfig
  #shapes = new Map<string, InternalShapeConfig>()
  #handles = new Map<string, ShapeHandle>()
  #streams = new Map<string, ShapeStream>()
  #abortControllers = new Map<string, AbortController>()
  #scripts: ScriptShaCache | null = null
  #isRunning = false

  constructor(config: ElectricRedisConfig) {
    this.#config = {
      batchSize: 1000,
      ...config,
      redis: config.redis as unknown as RedisClientLike,
    }
  }

  /**
   * Sync a shape to Redis and return a handle for event subscriptions
   *
   * @param name - Unique name for this shape (also used as Redis key)
   * @param config - Shape configuration
   * @returns A handle for subscribing to insert/update/delete events
   *
   * @example
   * ```typescript
   * // Hash structure (default)
   * const users = sync.syncShape<User>('users', {
   *   shape: { table: 'users' }
   * })
   *
   * // Set structure - keys only
   * const activeIds = sync.syncShape('active', {
   *   shape: { table: 'users' },
   *   structure: 'set'
   * })
   *
   * // Sorted set - ordered by score
   * const leaderboard = sync.syncShape<Score>('scores', {
   *   shape: { table: 'scores' },
   *   structure: 'sorted_set',
   *   score: 'points'
   * })
   *
   * // Stream - append-only changelog
   * const changelog = sync.syncShape<Item>('changes', {
   *   shape: { table: 'items' },
   *   structure: 'stream'
   * })
   * ```
   */
  syncShape<T = unknown>(
    name: string,
    config: ShapeConfig | Omit<HashShapeConfig, `structure`>
  ): ShapeHandle<T> {
    if (this.#isRunning) {
      throw new Error(
        `Cannot add shapes while sync is running. Call stop() first.`
      )
    }

    if (!config.shape?.table) {
      throw new Error(`Shape config must specify shape.table`)
    }

    const structure = (config as ShapeConfig).structure || `hash`

    // Validate sorted_set config - score is REQUIRED
    if (structure === `sorted_set`) {
      const sortedSetConfig = config as SortedSetShapeConfig
      if (!sortedSetConfig.score) {
        throw new Error(
          `Shape '${name}' uses sorted_set but 'score' field is not specified. ` +
            `sorted_set requires a 'score' field that maps to a numeric column.`
        )
      }
    }

    const internalConfig: InternalShapeConfig = {
      shape: config.shape,
      structure,
      key: config.key || `id`,
      score:
        structure === `sorted_set`
          ? (config as SortedSetShapeConfig).score
          : undefined,
    }

    this.#shapes.set(name, internalConfig)

    const handle = new ShapeHandle<T>(name, name)
    this.#handles.set(name, handle as ShapeHandle)

    return handle
  }

  /**
   * Start synchronizing all defined shapes
   */
  async start(): Promise<void> {
    if (this.#isRunning) {
      throw new Error(`ElectricRedis is already running`)
    }

    if (this.#shapes.size === 0) {
      throw new Error(`No shapes defined. Call syncShape() first.`)
    }

    this.#isRunning = true

    try {
      // Connect to Redis (ignore "already connected" errors)
      try {
        await this.#config.redis.connect()
      } catch (connectError) {
        const errorMessage = String(connectError).toLowerCase()
        if (
          !errorMessage.includes(`already connect`) &&
          !errorMessage.includes(`socket already opened`)
        ) {
          throw connectError
        }
      }

      // Load Lua scripts
      await this.#loadScripts()

      // Start each shape stream
      for (const [name, config] of this.#shapes) {
        await this.#startShapeSync(name, config)
      }

      console.log(`ElectricRedis started with ${this.#shapes.size} shapes`)
    } catch (error) {
      this.#isRunning = false
      console.error(`Failed to start ElectricRedis:`, error)
      throw error
    }
  }

  /**
   * Stop synchronizing
   */
  async stop(): Promise<void> {
    if (!this.#isRunning) {
      return
    }

    for (const controller of this.#abortControllers.values()) {
      controller.abort()
    }

    for (const stream of this.#streams.values()) {
      stream.unsubscribeAll()
    }

    this.#abortControllers.clear()
    this.#streams.clear()
    this.#isRunning = false

    console.log(`ElectricRedis stopped`)
  }

  async #loadScripts(): Promise<void> {
    try {
      this.#scripts = await loadAllScripts(this.#config.redis)
    } catch (error) {
      throw new Error(`Failed to load Redis scripts: ${error}`)
    }
  }

  async #startShapeSync(
    name: string,
    config: InternalShapeConfig
  ): Promise<void> {
    await this.#clearShape(name)

    const abortController = new AbortController()
    this.#abortControllers.set(name, abortController)

    const { ...electricOptions } = this.#config.electric

    const streamOptions: ShapeStreamOptions = {
      ...electricOptions,
      params: { ...config.shape },
      signal: abortController.signal,
    }

    const stream = new ShapeStream(streamOptions)

    stream.subscribe(async (messages: Message[]) => {
      await this.#handleMessages(name, config, messages)
    })

    this.#streams.set(name, stream)
  }

  async #clearShape(name: string): Promise<void> {
    await this.#config.redis.del(name)
  }

  async #handleMessages(
    shapeName: string,
    config: InternalShapeConfig,
    messages: Message[]
  ): Promise<void> {
    if (!this.#isRunning || messages.length === 0) {
      return
    }

    const batches = chunk(messages, this.#config.batchSize)

    for (const batch of batches) {
      await this.#processBatch(shapeName, config, batch)
    }
  }

  async #processBatch(
    shapeName: string,
    config: InternalShapeConfig,
    messages: Message[]
  ): Promise<void> {
    const pipeline = this.#config.redis.multi()
    const handle = this.#handles.get(shapeName)

    if (!this.#scripts) {
      throw new Error(`Scripts not loaded. Call start() first.`)
    }

    for (const message of messages) {
      if (!isChangeMessage(message)) continue

      const key = String(message.key)
      const data = message.value

      handleOperation(
        pipeline,
        config.structure,
        message.headers.operation,
        key,
        data,
        shapeName,
        this.#scripts,
        config
      )

      if (handle) {
        try {
          handle.trigger(message.headers.operation, key, data)
        } catch (error) {
          console.error(
            `Error in callback for '${message.headers.operation}':`,
            error
          )
        }
      }
    }

    try {
      await pipeline.exec()
      console.log(
        `Redis updated successfully with ${messages.length} shape updates`
      )
    } catch (error) {
      console.error(`Error while updating Redis:`, error)
      throw error
    }
  }
}
