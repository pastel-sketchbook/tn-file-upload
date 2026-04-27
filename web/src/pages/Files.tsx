import {
	ChevronLeft,
	ChevronRight,
	Download,
	FileText,
	Play,
	Trash2,
	X,
} from 'lucide-react'
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
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

function isPreviewable(contentType: string): boolean {
	return (
		contentType.startsWith('image/') ||
		contentType.startsWith('video/') ||
		contentType.startsWith('audio/') ||
		contentType === 'application/pdf' ||
		contentType.startsWith('text/')
	)
}

// --- Slideshow Modal ---

function SlideshowModal({
	files,
	initialIndex,
	onClose,
}: {
	files: FileMeta[]
	initialIndex: number
	onClose: () => void
}) {
	const [currentIndex, setCurrentIndex] = useState(initialIndex)
	const [isAutoPlaying, setIsAutoPlaying] = useState(false)
	const autoPlayRef = useRef<ReturnType<typeof setInterval> | null>(null)

	const file = files[currentIndex]
	const total = files.length

	const goTo = useCallback(
		(index: number) => {
			if (index < 0 || index >= total) return
			setCurrentIndex(index)
		},
		[total],
	)

	const goNext = useCallback(() => {
		goTo((currentIndex + 1) % total)
	}, [currentIndex, total, goTo])

	const goPrev = useCallback(() => {
		goTo((currentIndex - 1 + total) % total)
	}, [currentIndex, total, goTo])

	// Keyboard navigation
	useEffect(() => {
		const handler = (e: KeyboardEvent) => {
			if (e.key === 'Escape') onClose()
			else if (e.key === 'ArrowRight' || e.key === ' ') goNext()
			else if (e.key === 'ArrowLeft') goPrev()
		}
		document.addEventListener('keydown', handler)
		return () => document.removeEventListener('keydown', handler)
	}, [onClose, goNext, goPrev])

	// Auto-play
	useEffect(() => {
		if (isAutoPlaying) {
			autoPlayRef.current = setInterval(goNext, 3000)
		}
		return () => {
			if (autoPlayRef.current) clearInterval(autoPlayRef.current)
		}
	}, [isAutoPlaying, goNext])

	return (
		<div
			className="fixed inset-0 z-50 flex items-center justify-center bg-black/70 backdrop-blur-md"
			onClick={onClose}
			onKeyDown={() => {}}
			role="dialog"
			aria-modal="true"
			aria-label={`Slideshow: ${file.fileName}`}
		>
			<div
				className="relative w-full max-w-[80%] h-[88vh] bg-surface/95 rounded-[var(--radius-xl)] border border-border/50 shadow-2xl overflow-hidden flex flex-col backdrop-blur-sm animate-[modalIn_0.3s_ease-out]"
				onClick={(e) => e.stopPropagation()}
				role="document"
				onKeyDown={() => {}}
			>
				{/* Header */}
				<div className="flex items-center justify-between px-6 py-3 border-b border-border/50 bg-surface/80 backdrop-blur-sm">
					<div className="flex items-center gap-3 min-w-0">
						<span className="text-xs font-medium text-accent bg-accent-light px-2.5 py-1 rounded-full">
							{currentIndex + 1} / {total}
						</span>
						<div className="min-w-0">
							<p className="font-medium text-sm text-text-primary truncate">
								{file.fileName}
							</p>
							<p className="text-xs text-text-muted">
								{file.contentType} — {formatBytes(file.sizeBytes)}
							</p>
						</div>
					</div>
					<div className="flex items-center gap-2">
						{total > 1 && (
							<button
								type="button"
								onClick={() => setIsAutoPlaying(!isAutoPlaying)}
								className={`p-2 rounded-[var(--radius-md)] transition-colors ${
									isAutoPlaying
										? 'text-accent bg-accent-light'
										: 'text-text-muted hover:text-text-primary hover:bg-surface-alt'
								}`}
								title={isAutoPlaying ? 'Stop slideshow' : 'Auto-play'}
							>
								<Play className="w-4 h-4" />
							</button>
						)}
						<button
							type="button"
							onClick={onClose}
							className="p-2 rounded-[var(--radius-md)] text-text-muted hover:text-text-primary hover:bg-surface-alt transition-colors"
						>
							<X className="w-5 h-5" />
						</button>
					</div>
				</div>

				{/* Content area with slide animation */}
				<div className="flex-1 relative overflow-hidden">
					<SlideContent file={file} />
				</div>

				{/* Navigation arrows */}
				{total > 1 && (
					<>
						<button
							type="button"
							onClick={goPrev}
							className="absolute left-3 top-1/2 -translate-y-1/2 w-10 h-10 rounded-full bg-surface/90 border border-border/50 shadow-lg flex items-center justify-center text-text-muted hover:text-accent hover:border-accent/30 hover:scale-110 transition-all backdrop-blur-sm"
							title="Previous (Left arrow)"
						>
							<ChevronLeft className="w-5 h-5" />
						</button>
						<button
							type="button"
							onClick={goNext}
							className="absolute right-3 top-1/2 -translate-y-1/2 w-10 h-10 rounded-full bg-surface/90 border border-border/50 shadow-lg flex items-center justify-center text-text-muted hover:text-accent hover:border-accent/30 hover:scale-110 transition-all backdrop-blur-sm"
							title="Next (Right arrow)"
						>
							<ChevronRight className="w-5 h-5" />
						</button>
					</>
				)}

				{/* Bottom dots / thumbnails */}
				{total > 1 && (
					<div className="flex items-center justify-center gap-2 px-6 py-3 border-t border-border/50 bg-surface/80 backdrop-blur-sm">
						{files.map((f, i) => (
							<button
								type="button"
								key={f.fileId}
								onClick={() => goTo(i)}
								className={`w-2.5 h-2.5 rounded-full transition-all duration-300 ${
									i === currentIndex
										? 'bg-accent w-7 rounded-full'
										: 'bg-text-muted/20 hover:bg-text-muted/40'
								}`}
								title={f.fileName}
							/>
						))}
					</div>
				)}
			</div>
		</div>
	)
}

function SlideContent({ file }: { file: FileMeta }) {
	const [objectUrl, setObjectUrl] = useState<string | null>(null)
	const [textContent, setTextContent] = useState<string | null>(null)
	const [loading, setLoading] = useState(true)
	const [ready, setReady] = useState(false)
	const [tiles, setTiles] = useState<number[]>([])

	// Generate randomized strip reveal order
	useEffect(() => {
		const stripCount = 16
		const indices = Array.from({ length: stripCount }, (_, i) => i)
		for (let i = indices.length - 1; i > 0; i--) {
			const j = Math.floor(Math.random() * (i + 1))
			;[indices[i], indices[j]] = [indices[j], indices[i]]
		}
		setTiles(indices)
	}, [])

	useEffect(() => {
		let cancelled = false
		let url: string | null = null
		setLoading(true)
		setReady(false)
		setObjectUrl(null)
		setTextContent(null)

		downloadFile(file.fileId)
			.then((blob) => {
				if (cancelled) return
				if (file.contentType.startsWith('text/')) {
					blob.text().then((text) => {
						if (!cancelled) {
							setTextContent(text)
							setLoading(false)
							requestAnimationFrame(() => {
								requestAnimationFrame(() => {
									if (!cancelled) setReady(true)
								})
							})
						}
					})
				} else if (file.contentType.startsWith('image/')) {
					url = URL.createObjectURL(blob)
					const img = new Image()
					img.onload = () => {
						if (!cancelled) {
							setObjectUrl(url)
							setLoading(false)
							requestAnimationFrame(() => {
								requestAnimationFrame(() => {
									if (!cancelled) setReady(true)
								})
							})
						}
					}
					img.src = url
				} else {
					url = URL.createObjectURL(blob)
					setObjectUrl(url)
					setLoading(false)
					requestAnimationFrame(() => {
						requestAnimationFrame(() => {
							if (!cancelled) setReady(true)
						})
					})
				}
			})
			.catch(() => {
				if (!cancelled) {
					toast.error('Failed to load preview')
					setLoading(false)
				}
			})

		return () => {
			cancelled = true
			if (url) URL.revokeObjectURL(url)
		}
	}, [file.fileId, file.contentType])

	const stripCount = 16

	return (
		<div className="absolute inset-0 flex items-center justify-center p-8">
			{loading ? (
				<div className="flex flex-col items-center gap-3 animate-[fadeIn_0.2s_ease-out]">
					<div className="w-10 h-10 border-3 border-accent/20 border-t-accent rounded-full animate-spin" />
					<p className="text-sm text-text-muted">Loading preview...</p>
				</div>
			) : (
				<div className="relative w-full h-full flex items-center justify-center">
					{/* Strip overlay */}
					<div className="absolute inset-0 z-10 flex pointer-events-none">
						{tiles.map((orderIndex, _stripIndex) => (
							<div
								key={`strip-${orderIndex}`}
								className="h-full transition-opacity ease-[cubic-bezier(0.16,1,0.3,1)]"
								style={{
									width: `${100 / stripCount}%`,
									backgroundColor: 'var(--color-surface)',
									opacity: ready ? 0 : 1,
									transitionDuration: '400ms',
									transitionDelay: ready ? `${orderIndex * 30}ms` : '0ms',
								}}
							/>
						))}
					</div>

					{/* Actual content underneath */}
					<div className="w-full h-full flex items-center justify-center">
						{textContent !== null ? (
							<pre className="w-full h-full overflow-auto text-sm text-text-primary font-mono bg-surface-alt/80 p-6 rounded-[var(--radius-lg)] whitespace-pre-wrap break-words border border-border/30">
								{textContent}
							</pre>
						) : file.contentType.startsWith('image/') && objectUrl ? (
							<img
								src={objectUrl}
								alt={file.fileName}
								className="max-w-full max-h-full object-contain rounded-[var(--radius-lg)] shadow-lg"
							/>
						) : file.contentType.startsWith('video/') && objectUrl ? (
							<video
								src={objectUrl}
								controls
								className="max-w-full max-h-full rounded-[var(--radius-lg)] shadow-lg"
							>
								<track kind="captions" />
							</video>
						) : file.contentType.startsWith('audio/') && objectUrl ? (
							<div className="flex flex-col items-center gap-6">
								<div className="w-32 h-32 rounded-full bg-accent-light flex items-center justify-center">
									<FileText className="w-16 h-16 text-accent" />
								</div>
								<audio src={objectUrl} controls className="w-80">
									<track kind="captions" />
								</audio>
							</div>
						) : file.contentType === 'application/pdf' && objectUrl ? (
							<iframe
								src={objectUrl}
								title={file.fileName}
								className="w-full h-full rounded-[var(--radius-lg)] border border-border/30"
							/>
						) : (
							<div className="flex flex-col items-center gap-3">
								<FileText className="w-16 h-16 text-text-muted/30" />
								<p className="text-text-muted text-sm">
									Preview not available for this file type.
								</p>
							</div>
						)}
					</div>
				</div>
			)}
		</div>
	)
}

// --- Main Page ---

export function FilesPage() {
	const [files, setFiles] = useState<FileMeta[]>([])
	const [loading, setLoading] = useState(true)
	const [slideshowIndex, setSlideshowIndex] = useState<number | null>(null)

	const previewableFiles = useMemo(
		() => files.filter((f) => isPreviewable(f.contentType)),
		[files],
	)

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

	const openPreview = (file: FileMeta) => {
		const index = previewableFiles.findIndex((f) => f.fileId === file.fileId)
		if (index >= 0) {
			setSlideshowIndex(index)
		} else {
			toast('No preview for this file type')
		}
	}

	if (loading) {
		return (
			<div className="flex justify-center py-20">
				<div className="w-6 h-6 border-2 border-accent/30 border-t-accent rounded-full animate-spin" />
			</div>
		)
	}

	if (files.length === 0) {
		return (
			<div className="text-center py-20">
				<div className="w-16 h-16 mx-auto mb-4 rounded-[var(--radius-xl)] bg-surface-alt flex items-center justify-center">
					<FileText className="w-8 h-8 text-text-muted/40" />
				</div>
				<p className="text-lg font-medium text-text-primary">No files yet</p>
				<p className="mt-1 text-sm text-text-secondary">
					Upload a file to see it here.
				</p>
			</div>
		)
	}

	return (
		<div className="space-y-6">
			<div className="flex items-center justify-between">
				<div>
					<h1 className="font-serif text-2xl font-semibold text-text-primary">
						Files
					</h1>
					<p className="mt-1 text-sm text-text-secondary">
						{files.length} file{files.length !== 1 ? 's' : ''} uploaded
						{previewableFiles.length > 0 && (
							<span>
								{' '}
								—{' '}
								<button
									type="button"
									onClick={() => setSlideshowIndex(0)}
									className="text-accent hover:underline"
								>
									slideshow ({previewableFiles.length})
								</button>
							</span>
						)}
					</p>
				</div>
				<button
					type="button"
					onClick={refresh}
					className="px-3 py-1.5 text-sm text-text-muted hover:text-text-primary border border-border rounded-[var(--radius-md)] hover:bg-surface-alt transition-colors"
				>
					Refresh
				</button>
			</div>

			<div className="grid gap-3">
				{files.map((f) => (
					<div
						key={f.fileId}
						className="flex items-center gap-4 p-4 bg-surface rounded-[var(--radius-lg)] border border-border shadow-[var(--shadow-sm)] hover:shadow-[var(--shadow-md)] transition-shadow"
					>
						<button
							type="button"
							onClick={() => openPreview(f)}
							className={`flex-shrink-0 w-10 h-10 rounded-[var(--radius-md)] flex items-center justify-center transition-all ${
								isPreviewable(f.contentType)
									? 'bg-accent-light cursor-pointer hover:bg-accent/20 hover:scale-110'
									: 'bg-surface-alt cursor-default'
							}`}
							title={
								isPreviewable(f.contentType)
									? 'Preview'
									: 'No preview available'
							}
						>
							<FileText
								className={`w-5 h-5 ${isPreviewable(f.contentType) ? 'text-accent' : 'text-text-muted/40'}`}
							/>
						</button>
						<div className="flex-1 min-w-0">
							<p className="font-medium text-sm text-text-primary truncate">
								{f.fileName}
							</p>
							<div className="flex items-center gap-3 mt-0.5">
								<span className="text-xs text-text-muted">
									{formatBytes(f.sizeBytes)}
								</span>
								<span className="text-xs text-border-medium">|</span>
								<span className="text-xs text-text-muted">
									{formatDate(f.uploadedAt)}
								</span>
								<span className="text-xs text-border-medium">|</span>
								<code className="text-xs text-text-muted font-mono">
									{f.sha256Checksum}
								</code>
							</div>
						</div>
						<div className="flex items-center gap-1">
							<button
								type="button"
								onClick={() => handleDownload(f.fileId, f.fileName)}
								className="p-2 rounded-[var(--radius-md)] text-text-muted hover:text-accent hover:bg-accent-light transition-colors"
								title="Download"
							>
								<Download className="w-4 h-4" />
							</button>
							<button
								type="button"
								onClick={() => handleDelete(f.fileId)}
								className="p-2 rounded-[var(--radius-md)] text-text-muted hover:text-danger hover:bg-danger-light transition-colors"
								title="Delete"
							>
								<Trash2 className="w-4 h-4" />
							</button>
						</div>
					</div>
				))}
			</div>

			{slideshowIndex !== null && (
				<SlideshowModal
					files={previewableFiles}
					initialIndex={slideshowIndex}
					onClose={() => setSlideshowIndex(null)}
				/>
			)}
		</div>
	)
}
