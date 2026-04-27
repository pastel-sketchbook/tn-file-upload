import { CheckCircle, CloudUpload, File as FileIcon, X } from 'lucide-react'
import { useCallback, useState } from 'react'
import toast from 'react-hot-toast'
import { type UploadResult, uploadFile } from '@/api/client'

function formatBytes(bytes: number): string {
	if (bytes < 1024) return `${bytes} B`
	if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
	return `${(bytes / (1024 * 1024)).toFixed(2)} MB`
}

interface FileEntry {
	id: string
	file: File
	status: 'pending' | 'uploading' | 'done' | 'error'
	result?: UploadResult
	error?: string
}

let nextId = 0
function genId(): string {
	return `file-${++nextId}-${Date.now()}`
}

export function UploadPage() {
	const [files, setFiles] = useState<FileEntry[]>([])
	const [dragOver, setDragOver] = useState(false)

	const addFiles = useCallback((newFiles: FileList | File[]) => {
		const entries: FileEntry[] = Array.from(newFiles).map((file) => ({
			id: genId(),
			file,
			status: 'pending',
		}))
		setFiles((prev) => [...prev, ...entries])
	}, [])

	const handleDrop = useCallback(
		(e: React.DragEvent) => {
			e.preventDefault()
			setDragOver(false)
			if (e.dataTransfer.files.length > 0) {
				addFiles(e.dataTransfer.files)
			}
		},
		[addFiles],
	)

	const removeFile = (id: string) => {
		setFiles((prev) => prev.filter((f) => f.id !== id))
	}

	const handleUploadAll = async () => {
		const pending = files.filter((f) => f.status === 'pending')
		if (pending.length === 0) return

		for (const entry of files) {
			if (entry.status !== 'pending') continue

			setFiles((prev) =>
				prev.map((f) =>
					f.id === entry.id ? { ...f, status: 'uploading' } : f,
				),
			)

			try {
				const result = await uploadFile(entry.file)
				setFiles((prev) =>
					prev.map((f) =>
						f.id === entry.id ? { ...f, status: 'done', result } : f,
					),
				)
			} catch (err) {
				const msg = err instanceof Error ? err.message : 'Upload failed'
				setFiles((prev) =>
					prev.map((f) =>
						f.id === entry.id ? { ...f, status: 'error', error: msg } : f,
					),
				)
			}
		}

		toast.success(
			`Uploaded ${pending.length} file${pending.length !== 1 ? 's' : ''}`,
		)
	}

	const pendingCount = files.filter((f) => f.status === 'pending').length
	const doneCount = files.filter((f) => f.status === 'done').length
	const isUploading = files.some((f) => f.status === 'uploading')

	return (
		<div className="space-y-8">
			<div>
				<h1 className="font-serif text-2xl font-semibold text-text-primary">
					Upload Files
				</h1>
				<p className="mt-1 text-sm text-text-secondary">
					Upload files up to 100 MB. They are chunked and checksummed with
					SHA-256.
				</p>
			</div>

			<label
				onDrop={handleDrop}
				onDragOver={(e) => {
					e.preventDefault()
					setDragOver(true)
				}}
				onDragLeave={() => setDragOver(false)}
				className={`relative flex flex-col items-center justify-center rounded-[var(--radius-xl)] border-2 border-dashed p-16 cursor-pointer transition-all ${
					dragOver
						? 'border-accent bg-accent-lighter scale-[1.01] shadow-[var(--shadow-md)]'
						: 'border-border-medium bg-surface hover:border-accent/40 hover:bg-accent-lighter/50'
				}`}
			>
				<CloudUpload
					className={`w-14 h-14 mb-4 transition-colors ${dragOver ? 'text-accent' : 'text-text-muted/40'}`}
				/>
				<p className="text-base font-medium text-text-primary">
					Drag and drop files here
				</p>
				<p className="mt-1 text-sm text-text-muted">
					or click to browse — multiple files supported
				</p>
				<input
					type="file"
					multiple
					className="absolute inset-0 w-full h-full opacity-0 cursor-pointer"
					onChange={(e) => {
						if (e.target.files && e.target.files.length > 0) {
							addFiles(e.target.files)
						}
					}}
				/>
			</label>

			{files.length > 0 && (
				<div className="space-y-3">
					<div className="flex items-center justify-between">
						<p className="text-sm text-text-secondary">
							{files.length} file{files.length !== 1 ? 's' : ''}
							{doneCount > 0 && ` — ${doneCount} uploaded`}
						</p>
						<div className="flex gap-2">
							{files.length > 0 && !isUploading && (
								<button
									type="button"
									onClick={() => setFiles([])}
									className="px-3 py-1.5 text-sm text-text-muted hover:text-text-primary border border-border rounded-[var(--radius-md)] hover:bg-surface-alt transition-colors"
								>
									Clear
								</button>
							)}
							{pendingCount > 0 && (
								<button
									type="button"
									onClick={handleUploadAll}
									disabled={isUploading}
									className={`px-5 py-2 rounded-[var(--radius-md)] text-sm font-medium text-white transition-all ${
										isUploading
											? 'bg-accent/60 cursor-not-allowed'
											: 'bg-accent hover:bg-accent-hover shadow-[var(--shadow-sm)] hover:shadow-[var(--shadow-md)]'
									}`}
								>
									{isUploading ? (
										<span className="flex items-center gap-2">
											<span className="w-4 h-4 border-2 border-white/30 border-t-white rounded-full animate-spin" />
											Uploading...
										</span>
									) : (
										`Upload ${pendingCount} file${pendingCount !== 1 ? 's' : ''}`
									)}
								</button>
							)}
						</div>
					</div>

					<div className="space-y-2">
						{files.map((entry) => (
							<div
								key={entry.id}
								className="flex items-center gap-3 p-3 bg-surface rounded-[var(--radius-lg)] border border-border shadow-[var(--shadow-sm)]"
							>
								<div
									className={`flex-shrink-0 w-9 h-9 rounded-[var(--radius-md)] flex items-center justify-center ${
										entry.status === 'done'
											? 'bg-success-light'
											: entry.status === 'error'
												? 'bg-danger-light'
												: 'bg-accent-light'
									}`}
								>
									{entry.status === 'done' ? (
										<CheckCircle className="w-4 h-4 text-success" />
									) : (
										<FileIcon
											className={`w-4 h-4 ${entry.status === 'error' ? 'text-danger' : 'text-accent'}`}
										/>
									)}
								</div>
								<div className="flex-1 min-w-0">
									<p className="font-medium text-sm text-text-primary truncate">
										{entry.file.name}
									</p>
									<p className="text-xs text-text-muted">
										{formatBytes(entry.file.size)}
										{entry.status === 'done' && entry.result && (
											<span className="ml-2 text-success">
												— {entry.result.checksum.slice(0, 12)}...
											</span>
										)}
										{entry.status === 'error' && (
											<span className="ml-2 text-danger">{entry.error}</span>
										)}
									</p>
								</div>
								{entry.status === 'uploading' && (
									<span className="w-4 h-4 border-2 border-accent/30 border-t-accent rounded-full animate-spin" />
								)}
								{entry.status === 'pending' && (
									<button
										type="button"
										onClick={() => removeFile(entry.id)}
										className="p-1.5 rounded-[var(--radius-sm)] text-text-muted hover:text-danger hover:bg-danger-light transition-colors"
									>
										<X className="w-4 h-4" />
									</button>
								)}
							</div>
						))}
					</div>
				</div>
			)}
		</div>
	)
}
