import { Outlet } from "react-router-dom";
import { DocumentMeta } from "./document-meta";
import { Header } from "./header";

export function Layout() {
  return (
    <div className="flex min-h-screen flex-col">
      <DocumentMeta />
      <Header />
      <Outlet />
    </div>
  );
}
