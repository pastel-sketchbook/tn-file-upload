import { createFileRoute } from '@tanstack/react-router'
import { FilesPage } from '@/pages/Files'

export const Route = createFileRoute('/files')({
	component: FilesPage,
})
