import { Outlet } from 'react-router-dom'
import { Header } from './header'

export function Layout() {
  return (
    <div className="flex min-h-screen flex-col">
      <Header />
      <main className="mx-auto w-full max-w-[1100px] flex-1 p-5">
        <Outlet />
      </main>
    </div>
  )
}
