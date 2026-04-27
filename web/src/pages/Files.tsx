import { Download, FileText, Trash2 } from 'lucide-react'
import { useCallback, useEffect, useState } from 'react'
import toast from 'react-hot-toast'
import {
	deleteFile,
	downloadFile,
	type FileMeta,
	listFiles,
} from '@/api/client'

function formatBytes(bytes: number): string {
	if (bytes < 1024) return `${bytes} B`
	if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
	return `${(bytes / (1024 * 1024)).toFixed(2)} MB`
}

function formatDate(iso: string): string {
	try {
		return new Date(iso).toLocaleDateString(undefined, {
			month: 'short',
			day: 'numeric',
			hour: '2-digit',
			minute: '2-digit',
		})
	} catch {
		return iso
	}
}

export function FilesPage() {
	const [files, setFiles] = useState<FileMeta[]>([])
	const [loading, setLoading] = useState(true)

	const refresh = useCallback(async () => {
		setLoading(true)
		try {
			const data = await listFiles()
			setFiles(data)
		} catch {
			toast.error('Failed to load files')
		} finally {
			setLoading(false)
		}
	}, [])

	useEffect(() => {
		refresh()
	}, [refresh])

	const handleDelete = async (fileId: string) => {
		try {
			await deleteFile(fileId)
			toast.success('File deleted')
			refresh()
		} catch {
			toast.error('Delete failed')
		}
	}

	const handleDownload = async (fileId: string, fileName: string) => {
		try {
			const blob = await downloadFile(fileId)
			const url = URL.createObjectURL(blob)
			const a = document.createElement('a')
			a.href = url
			a.download = fileName
			a.click()
			URL.revokeObjectURL(url)
		} catch {
			toast.error('Download failed')
		}
	}

	if (loading) {
		return (
			<div className="flex justify-center py-20">
				<div className="w-6 h-6 border-2 border-primary/30 border-t-primary rounded-full animate-spin" />
			</div>
		)
	}

	if (files.length === 0) {
		return (
			<div className="text-center py-20">
				<div className="w-16 h-16 mx-auto mb-4 rounded-2xl bg-gray-100 flex items-center justify-center">
					<FileText className="w-8 h-8 text-gray-300" />
				</div>
				<p className="text-lg font-medium text-gray-600">No files yet</p>
				<p className="mt-1 text-sm text-muted">Upload a file to see it here.</p>
			</div>
		)
	}

	return (
		<div className="space-y-6">
			<div className="flex items-center justify-between">
				<div>
					<h1 className="text-2xl font-bold text-gray-900">Files</h1>
					<p className="mt-1 text-sm text-muted">
						{files.length} file{files.length !== 1 ? 's' : ''} uploaded
					</p>
				</div>
				<button
					type="button"
					onClick={refresh}
					className="px-3 py-1.5 text-sm text-muted hover:text-gray-900 border border-border rounded-lg hover:bg-gray-50 transition-colors"
				>
					Refresh
				</button>
			</div>

			<div className="grid gap-3">
				{files.map((f) => (
					<div
						key={f.fileId}
						className="flex items-center gap-4 p-4 bg-surface rounded-xl border border-border hover:shadow-sm transition-shadow"
					>
						<div className="flex-shrink-0 w-10 h-10 rounded-lg bg-primary-light flex items-center justify-center">
							<FileText className="w-5 h-5 text-primary" />
						</div>
						<div className="flex-1 min-w-0">
							<p className="font-medium text-sm text-gray-900 truncate">
								{f.fileName}
							</p>
							<div className="flex items-center gap-3 mt-0.5">
								<span className="text-xs text-muted">
									{formatBytes(f.sizeBytes)}
								</span>
								<span className="text-xs text-gray-300">|</span>
								<span className="text-xs text-muted">
									{formatDate(f.uploadedAt)}
								</span>
								<span className="text-xs text-gray-300">|</span>
								<code className="text-xs text-muted font-mono">
									{f.sha256Checksum.slice(0, 12)}...
								</code>
							</div>
						</div>
						<div className="flex items-center gap-1">
							<button
								type="button"
								onClick={() => handleDownload(f.fileId, f.fileName)}
								className="p-2 rounded-lg text-muted hover:text-primary hover:bg-primary-light transition-colors"
								title="Download"
							>
								<Download className="w-4 h-4" />
							</button>
							<button
								type="button"
								onClick={() => handleDelete(f.fileId)}
								className="p-2 rounded-lg text-muted hover:text-danger hover:bg-red-50 transition-colors"
								title="Delete"
							>
								<Trash2 className="w-4 h-4" />
							</button>
						</div>
					</div>
				))}
			</div>
		</div>
	)
}
