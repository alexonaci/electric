import { v4 as uuidv4 } from "uuid"
import {
  useElectricSync,
} from "@electric-sql/tanstack-react-query"
type Item = { id: string, age: number, description: string }

const baseUrl = import.meta.env.ELECTRIC_URL ?? `http://localhost:3000`
const baseApiUrl = `http://localhost:3001`
const itemsUrl = new URL(`/items`, baseApiUrl)

const itemShape = () => ({
  url: new URL(`/v1/shape`, baseUrl).href,
  params: {
    table: `items`,
  },
})

export function Example() {
  const { data, insert, deleteMany } = useElectricSync<Item>({
    shape: {
      options: itemShape(),
      id: 'id'
    },
    insertMutation: {
      mutationKey: ["add-item"],
      mutationFn: (newId) => fetch(itemsUrl, {
        method: "POST",
        body: JSON.stringify(newId),
      }),
    },
    deleteManyMutation: {
      mutationKey: ["clear-items"],
      mutationFn: (count: number) => fetch(itemsUrl, {
        method: "DELETE",
      }),
    },
  })

  // const { data, insert } = useInsertMutation<Item>({
  //   shapeConfig: {
  //     options: itemShape(),
  //     keyFn: (item) => item.id,
  //   },
  //   insertMutation: {
  //     mutationKey: [`add-item`],
  //     mutationFn: (newId) =>
  //       fetch(itemsUrl, {
  //         method: `POST`,
  //         body: JSON.stringify(newId),
  //       }),
  //   },
  // })

  // const { data: finalData, deleteMany } = useDeleteManyMutation<Item>({
  //   shapeConfig: {
  //     options: itemShape(),
  //     keyFn: (item) => item.id,
  //   },
  //   deleteManyMutation: {
  //     mutationKey: [`clear-items`],
  //     mutationFn: (count: number) =>
  //       fetch(itemsUrl, {
  //         method: `DELETE`,
  //       }),
  //   },
  //   removeKeysOnMutate: [`add-item`] as any,
  // })

  return (
    <div>
      <div>
        <button type="submit" onClick={() => insert.mutateAsync({ id: uuidv4() })}>
          Add
        </button>
        <button type="submit" onClick={() => deleteMany.mutateAsync(data.length)}>
          Clear
        </button>
      </div>

      {data.map((item, i) => (
        <p key={i}>
          <code>{item.id}</code>
        </p>
      ))}
    </div>
  )
}
