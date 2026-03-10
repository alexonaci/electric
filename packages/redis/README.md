# @electric-sql/redis

Redis integration for Electric SQL - sync database Shapes to Redis data structures in real-time.

## Installation

```bash
npm install @electric-sql/redis redis
```

## Quick Start

```typescript
import { createClient } from 'redis'
import { RedisShapeSync } from '@electric-sql/redis'

// Connect to Redis
const redisClient = createClient({ url: 'redis://localhost:6379' })
await redisClient.connect()

// Create and start sync
const sync = new RedisShapeSync(redisClient, {
  url: 'http://localhost:3000/v1/shape',
  params: { table: 'items' },
})

await sync.start()

// Subscribe to updates
sync.onUpdate((data, stats) => {
  console.log(`Synced ${data.length} items`)
  console.log(`Last batch: +${stats.insertsInBatch} -${stats.deletesInBatch}`)
})

// Get current data
const items = await sync.getData()
console.log('Current items:', items)
```

## Core Concepts

### Electric SQL Concepts

- **Shape**: A subset of your database table(s) that gets synced to clients
- **ShapeStream**: A real-time stream of changes (insert/update/delete) for a Shape
- **Messages**: Individual change events that flow through the stream

### Redis Concepts

- **Hash**: Key-value structure where each database record becomes a hash field
- **Pipelining**: Batching multiple Redis commands for optimal performance
- **Lua Scripts**: Atomic server-side operations for complex updates
- **Transactions**: Ensure multiple Redis operations execute atomically

## Configuration Options

```typescript
interface RedisSyncOptions {
  /** Prefix for Redis keys (default: 'electric') */
  keyPrefix?: string

  /** Redis data structure to use (default: 'hash') */
  dataStructure?: 'hash' | 'json'

  /** Maximum commands per transaction (default: 1000) */
  batchSize?: number

  /** Clear existing data when starting (default: true) */
  clearOnStart?: boolean

  /** Custom key generator function */
  keyGenerator?: (prefix: string, table: string, id: string | number) => string

  /** Error handler callback */
  onError?: (error: Error) => void

  /** Update callback - called after each batch */
  onUpdate?: (stats: SyncStats) => void
}
```

## Examples

### Basic Usage with Custom Configuration

```typescript
import { RedisShapeSync } from '@electric-sql/redis'

const sync = new RedisShapeSync(
  redisClient,
  {
    url: 'http://localhost:3000/v1/shape',
    params: {
      table: 'products',
      where: 'category = "electronics"',
    },
  },
  {
    keyPrefix: 'myapp',
    batchSize: 500,
    clearOnStart: false,
    onError: (error) => {
      console.error('Sync error:', error)
      // Custom error handling - maybe retry, alert, etc.
    },
    onUpdate: (stats) => {
      console.log(
        `Processed batch: ${stats.insertsInBatch} inserts, ${stats.updatesInBatch} updates`
      )
    },
  }
)
```

### Monitoring Sync Status

```typescript
sync.onStatus((event) => {
  switch (event.status) {
    case 'connecting':
      console.log('Connecting to Electric...')
      break
    case 'connected':
      console.log('Connected! Stats:', event.stats)
      break
    case 'syncing':
      console.log('Processing updates...')
      break
    case 'error':
      console.error('Sync error:', event.error)
      break
    case 'stopped':
      console.log('Sync stopped')
      break
  }
})
```

### Advanced: Custom Key Generation

```typescript
const sync = new RedisShapeSync(redisClient, shapeOptions, {
  keyGenerator: (prefix, table, id) => {
    // Custom key format: app:env:table:id
    return `${prefix}:prod:${table}:${id}`
  },
})
```

### Working with the Synced Data

```typescript
// Get all current data
const allItems = await sync.getData()

// Subscribe to real-time updates
const unsubscribe = sync.onUpdate(async (data, stats) => {
  // Data contains the complete current dataset
  console.log(`Total items: ${data.length}`)

  // Stats contains information about the last batch
  if (stats.insertsInBatch > 0) {
    console.log(`${stats.insertsInBatch} new items added`)
  }

  // You can also query Redis directly for specific items
  const specificItem = await redisClient.hGet('electric:items', '123')
  if (specificItem) {
    const item = JSON.parse(specificItem)
    console.log('Item 123:', item)
  }
})

// Later, stop listening for updates
unsubscribe()
```

## Redis Data Structure

When using the default 'hash' data structure, your data is stored as:

```
Key: {keyPrefix}:{tableName}
Field: {recordId}
Value: JSON.stringify(record)
```

Example:

```
HGETALL electric:items
1) "1"
2) "{\"id\":1,\"name\":\"Widget\",\"price\":19.99}"
3) "2"
4) "{\"id\":2,\"name\":\"Gadget\",\"price\":29.99}"
```

## Error Handling

The library includes comprehensive error handling:

```typescript
const sync = new RedisShapeSync(redisClient, shapeOptions, {
  onError: (error) => {
    if (error.message.includes('Redis connection')) {
      // Handle Redis connection issues
      console.log('Redis connection lost, will retry...')
    } else if (error.message.includes('Electric')) {
      // Handle Electric SQL issues
      console.log('Electric sync issue:', error)
    } else {
      // Handle other errors
      console.error('Unexpected error:', error)
    }
  },
})
```

The library automatically retries failed operations with exponential backoff and includes circuit breaker logic for resilience.

## Performance Considerations

### Batch Size

- Default batch size is 1000 Redis commands per transaction
- Increase for better throughput on stable connections
- Decrease to reduce memory usage and improve error recovery

### Memory Usage

- Each synced record is stored in Redis memory
- Monitor Redis memory usage as your dataset grows
- Consider using Redis clustering for large datasets

### Network Optimization

- Redis pipelining is used automatically for batch operations
- Lua scripts minimize round-trips for complex operations
- Connection pooling is supported via the Redis client

## TypeScript Support

Full TypeScript support with generic types:

```typescript
interface Product {
  id: number
  name: string
  price: number
  category: string
}

const sync = new RedisShapeSync<Product>(redisClient, {
  url: 'http://localhost:3000/v1/shape',
  params: { table: 'products' },
})

// data is typed as Product[]
sync.onUpdate((data: Product[], stats) => {
  data.forEach((product) => {
    console.log(`${product.name}: $${product.price}`) // TypeScript knows these fields exist
  })
})
```

## API Reference

### RedisShapeSync

#### Constructor

```typescript
new RedisShapeSync<T>(
  redisClient: RedisClientLike,
  shapeOptions: ShapeStreamOptions<T>,
  syncOptions?: RedisSyncOptions
)
```

#### Methods

- `async start(): Promise<void>` - Start syncing
- `stop(): void` - Stop syncing and cleanup
- `async getData(): Promise<T[]>` - Get current synced data
- `onUpdate(callback): UnsubscribeFn` - Subscribe to data updates
- `onStatus(callback): UnsubscribeFn` - Subscribe to status changes
- `getStats(): SyncStats` - Get current sync statistics

## Contributing

Contributions are welcome! Please see the [Electric SQL contributing guide](../../CONTRIBUTING.md) for details.

## License

Apache 2.0 - see [LICENSE](../../LICENSE) for details.
