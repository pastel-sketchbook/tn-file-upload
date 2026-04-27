import {
	createRootRoute,
	Link,
	Outlet,
	useLocation,
} from '@tanstack/react-router'
import { FolderOpen, Upload } from 'lucide-react'
import { Toaster } from 'react-hot-toast'

export const Route = createRootRoute({
	component: RootLayout,
})

function NavLink({
	to,
	children,
	icon: Icon,
}: {
	to: string
	children: React.ReactNode
	icon: React.ComponentType<{ className?: string }>
}) {
	const { pathname } = useLocation()
	const active = pathname === to

	return (
		<Link
			to={to}
			className={`flex items-center gap-2 px-4 py-2 rounded-[var(--radius-md)] text-sm font-medium transition-colors ${
				active
					? 'bg-accent-light text-accent'
					: 'text-text-muted hover:text-text-primary hover:bg-surface-alt'
			}`}
		>
			<Icon className="w-4 h-4" />
			{children}
		</Link>
	)
}

function RootLayout() {
	return (
		<div className="min-h-screen bg-gradient-to-br from-bg-gradient-from to-bg-gradient-to">
			<header className="sticky top-0 z-10 bg-surface/80 backdrop-blur-md border-b border-border">
				<div className="w-full max-w-[80%] mx-auto px-6 h-16 flex items-center">
					<Link to="/" className="flex items-center gap-3">
						<div className="w-8 h-8 rounded-[var(--radius-md)] bg-accent flex items-center justify-center">
							<Upload className="w-4 h-4 text-white" />
						</div>
						<span className="font-serif text-lg font-semibold text-text-primary">
							tn-file-upload
						</span>
					</Link>
					<nav className="ml-auto flex gap-1">
						<NavLink to="/" icon={Upload}>
							Upload
						</NavLink>
						<NavLink to="/files" icon={FolderOpen}>
							Files
						</NavLink>
					</nav>
				</div>
			</header>
			<main className="w-full max-w-[80%] mx-auto px-6 py-10">
				<Outlet />
			</main>
			<Toaster
				position="bottom-right"
				toastOptions={{
					style: {
						borderRadius: 'var(--radius-md)',
						background: '#2d3748',
						color: '#f8f9fc',
						fontSize: '14px',
						fontFamily: 'var(--font-sans)',
					},
				}}
			/>
		</div>
	)
}
