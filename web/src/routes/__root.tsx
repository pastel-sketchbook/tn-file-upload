import { createRootRoute, Link, Outlet } from '@tanstack/react-router'
import { Toaster } from 'react-hot-toast'

export const Route = createRootRoute({
	component: RootLayout,
})

function RootLayout() {
	return (
		<div className="min-h-screen bg-gray-50 text-gray-900">
			<header className="bg-white shadow-sm px-6 py-4 flex items-center">
				<Link to="/" className="text-xl font-bold">
					tn-file-upload
				</Link>
				<nav className="ml-auto flex gap-4">
					<Link to="/" className="hover:underline">
						Upload
					</Link>
					<Link to="/files" className="hover:underline">
						Files
					</Link>
				</nav>
			</header>
			<main className="container mx-auto p-6">
				<Outlet />
			</main>
			<Toaster position="bottom-right" />
		</div>
	)
}
