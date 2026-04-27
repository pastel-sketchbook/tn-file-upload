import { Download, FileText, Trash2 } from 'lucide-react'
import { useCallback, useEffect, useState } from 'react'
import toast from 'react-hot-toast'
import {
	deleteFile,
	downloadFile,
	type FileMeta,
	listFiles,
} from '@/api/client'

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
		return <p className="text-center text-base-content/60">Loading…</p>
	}

	if (files.length === 0) {
		return (
			<div className="text-center py-12">
				<FileText className="mx-auto w-12 h-12 text-base-content/30 mb-4" />
				<p className="text-lg text-base-content/60">No files uploaded yet</p>
			</div>
		)
	}

	return (
		<div className="max-w-4xl mx-auto space-y-4">
			<h1 className="text-3xl font-bold">Files</h1>
			<div className="overflow-x-auto">
				<table className="table table-zebra w-full">
					<thead>
						<tr>
							<th>Name</th>
							<th>Size</th>
							<th>Checksum</th>
							<th>Uploaded</th>
							<th>Actions</th>
						</tr>
					</thead>
					<tbody>
						{files.map((f) => (
							<tr key={f.fileId}>
								<td className="font-mono text-sm">{f.fileName}</td>
								<td>{(f.sizeBytes / 1024).toFixed(1)} KB</td>
								<td className="font-mono text-xs">
									{f.sha256Checksum.slice(0, 12)}…
								</td>
								<td className="text-sm">{f.uploadedAt}</td>
								<td className="flex gap-2">
									<button
										type="button"
										onClick={() => handleDownload(f.fileId, f.fileName)}
										className="btn btn-ghost btn-xs"
										title="Download"
									>
										<Download className="w-4 h-4" />
									</button>
									<button
										type="button"
										onClick={() => handleDelete(f.fileId)}
										className="btn btn-ghost btn-xs text-error"
										title="Delete"
									>
										<Trash2 className="w-4 h-4" />
									</button>
								</td>
							</tr>
						))}
					</tbody>
				</table>
			</div>
		</div>
	)
}
