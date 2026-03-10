/**
 * iovalkey Example - Valkey Compatibility Demo
 *
 * This example demonstrates using @electric-sql/redis with iovalkey client
 * connecting to a Valkey server (Redis fork).
 *
 * iovalkey is a fork of ioredis, so it has the same API.
 *
 * Run with:
 *   REDIS_URL=redis://localhost:6389 pnpm tsx src/iovalkey-example.ts
 *
 * Note: Port 6389 is for Valkey (see docker-compose.yml)
 */

import Valkey from 'iovalkey'
import { ElectricRedis } from '@electric-sql/redis'

interface Item {
  id: string
  name: string
  value: number
  created_at: string
}

async function main() {
  console.log(`ElectricRedis with iovalkey + Valkey Server`)
  console.log(`============================================\n`)

  // Connect to Valkey server (port 6389 in docker-compose)
  const valkey = new Valkey(process.env.REDIS_URL || `redis://localhost:6389`)

  valkey.on(`error`, (err) => console.error(`Valkey error:`, err))
  valkey.on(`connect`, () =>
    console.log(`Connected to Valkey server via iovalkey`)
  )

  const sync = new ElectricRedis({
    electric: {
      url: process.env.ELECTRIC_URL || `http://localhost:3000/v1/shape`,
    },
    redis: valkey,
  })

  // Sync items to a hash
  const items = sync.syncShape<Item>(`items`, {
    shape: { table: `items` },
    key: `id`,
    structure: `hash`, // default
  })

  items.onInsert((key, item) => {
    console.log(`[INSERT] ${item.name}: ${item.value}`)
  })

  items.onUpdate((key, item) => {
    console.log(`[UPDATE] ${JSON.stringify(item)}`)
  })

  items.onDelete((key) => {
    console.log(`[DELETE] ${key}`)
  })

  await sync.start()
  console.log(`Sync started!\n`)

  await new Promise((resolve) => setTimeout(resolve, 2000))

  // Read data using iovalkey client directly (lowercase methods like ioredis)
  const allItems = await valkey.hgetall(`items`)
  console.log(`\nItems in Valkey:`)
  for (const [key, value] of Object.entries(allItems)) {
    const item = JSON.parse(value)
    console.log(`  ${key}: ${item.name} = ${item.value}`)
  }

  // Get a single item
  const singleItem = await valkey.hget(`items`, Object.keys(allItems)[0])
  if (singleItem) {
    console.log(`\nSingle item:`, JSON.parse(singleItem))
  }

  console.log(`\nPress Ctrl+C to stop...`)

  const cleanup = async () => {
    await sync.stop()
    valkey.disconnect()
    process.exit(0)
  }

  process.on(`SIGINT`, cleanup)
  process.on(`SIGTERM`, cleanup)
}

main().catch((err) => {
  console.error(`Error:`, err)
  process.exit(1)
})
