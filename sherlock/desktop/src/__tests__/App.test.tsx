import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import App from "../App";
import * as api from "../api";
import { mockRoot } from "./fixtures";

vi.mock("../hooks/useToast", () => ({
  useToast: () => ({ notice: null, error: null, setNotice: vi.fn(), setError: vi.fn() }),
}));

vi.mock("../hooks/useUserConfig", () => ({
  useUserConfig: () => {},
}));

vi.mock("../hooks/useGridColumns", () => ({
  useGridColumns: () => ({ current: 4 }),
}));

vi.mock("../hooks/useInfiniteScroll", () => ({
  useInfiniteScroll: () => {},
}));

vi.mock("../hooks/usePolling", () => ({
  usePolling: () => {},
}));

vi.mock("../hooks/useSelection", () => ({
  useSelection: () => ({
    selectedIndices: new Set<number>(),
    focusIndex: null,
    anchorIndex: null,
    selectOnly: vi.fn(),
    toggleSelect: vi.fn(),
    rangeSelect: vi.fn(),
    selectAll: vi.fn(),
    clearSelection: vi.fn(),
    replaceSelection: vi.fn(),
  }),
}));

vi.mock("../hooks/useSearch", () => ({
  useSearch: () => ({
    items: [] as any[],
    total: 0,
    loading: false,
    loadingMore: false,
    canLoadMore: false,
    runSearch: vi.fn(() => Promise.resolve()),
    onLoadMore: vi.fn(),
  }),
}));

const mockInitApp = vi.fn(() => Promise.resolve(null));
const mockRefreshRoots = vi.fn(() => Promise.resolve());
const mockAddTrackedJobId = vi.fn();

vi.mock("../hooks/useScanManager", () => ({
  useScanManager: () => ({
    initApp: mockInitApp,
    refreshRoots: mockRefreshRoots,
    addTrackedJobId: mockAddTrackedJobId,
    trackedJobIds: [] as number[],
    completedJobs: [] as any[],
    setCompletedJobs: vi.fn(),
    activeScans: [] as any[],
    pollRuntimeAndScans: vi.fn(),
    onRescanRoot: vi.fn(),
    onRefreshRoot: vi.fn(),
    onPickAndScan: vi.fn(),
    onCancelScan: vi.fn(),
    onResumeScan: vi.fn(),
    onRecheckSetup: vi.fn(),
    onSetupDownload: vi.fn(),
    onSetupOcr: vi.fn(),
    onResumeAllInterrupted: vi.fn(),
  }),
}));

vi.mock("../hooks/useGridNavigation", () => ({
  useGridNavigation: () => {},
}));

vi.mock("../hooks/useAutoUpdate", () => ({
  useAutoUpdate: () => ({
    updateInfo: null,
    updateChecking: false,
    updateDownloading: false,
    updateProgress: null,
    checkForUpdates: vi.fn(),
    installUpdate: vi.fn(),
  }),
}));

vi.mock("../hooks/useFaceDetection", () => ({
  useFaceDetection: () => ({
    facesMode: false,
    setFacesMode: vi.fn(),
    faceProgress: null,
    onDetectFaces: vi.fn(),
    onCancelFaceDetect: vi.fn(),
  }),
}));

vi.mock("../hooks/useAlbumManager", () => ({
  useAlbumManager: () => ({
    albums: [] as any[],
    refreshAlbums: vi.fn(() => Promise.resolve()),
    onSelectAlbum: vi.fn(() => ({ query: "" })),
    onAddToAlbum: vi.fn(),
    onCreateAlbumFromSelection: vi.fn(),
    showCreateAlbum: false,
    closeCreateModal: vi.fn(),
    onCreateAlbumConfirm: vi.fn(),
    onDeleteAlbum: vi.fn(),
    onReorderAlbums: vi.fn(),
  }),
}));

vi.mock("../hooks/useSmartFolderManager", () => ({
  useSmartFolderManager: () => ({
    smartFolders: [] as any[],
    activeSmartFolderId: null,
    refreshSmartFolders: vi.fn(() => Promise.resolve()),
    setActiveSmartFolderId: vi.fn(),
    onSelectSmartFolder: vi.fn(() => ({ query: "" })),
    showCreateSmartFolder: false,
    closeCreateModal: vi.fn(),
    onCreateSmartFolderConfirm: vi.fn(),
    onDeleteSmartFolder: vi.fn(),
    onReorderSmartFolders: vi.fn(),
  }),
}));

vi.mock("../hooks/useDuplicatesManager", () => ({
  useDuplicatesManager: () => ({
    duplicatesMode: false,
    duplicatesData: null,
    duplicatesLoading: false,
    duplicatesSelected: new Set<number>(),
    nearEnabled: false,
    nearThreshold: 5,
    onNearEnabledChange: vi.fn(),
    onNearThresholdChange: vi.fn(),
    onToggleFile: vi.fn(),
    onSelectAllDuplicates: vi.fn(),
    onDeselectAll: vi.fn(),
    onFindDuplicates: vi.fn(),
    onBack: vi.fn(),
    onSelectGroupDuplicates: vi.fn(),
    onPreviewGroup: vi.fn(),
    setDuplicatesMode: vi.fn(),
    refreshAfterDelete: vi.fn(() => Promise.resolve()),
    dupPreviewItems: [] as any[],
    setDupPreviewItems: vi.fn(),
    getDeleteSearchItems: vi.fn(() => []),
  }),
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: vi.fn(() => ({
    close: vi.fn(),
    minimize: vi.fn(),
    toggleMaximize: vi.fn(),
    destroy: vi.fn(),
  })),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(),
}));

vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl: vi.fn(),
}));

function renderApp() {
  return render(<App />);
}

describe("App", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockInitApp.mockResolvedValue(null);
  });

  it("renders the app shell container", () => {
    const { container } = renderApp();
    expect(container.querySelector(".app-shell")).toBeInTheDocument();
  });

  it("renders the titlebar with app name", () => {
    renderApp();
    expect(screen.getByText("Frank Sherlock")).toBeInTheDocument();
  });

  it("renders the sidebar with empty message", () => {
    renderApp();
    expect(screen.getByText("No folders scanned yet")).toBeInTheDocument();
  });

  it("renders the content area with search query input", () => {
    renderApp();
    expect(screen.getByLabelText("Search query")).toBeInTheDocument();
  });

  it("renders the media type filter", () => {
    renderApp();
    expect(screen.getByLabelText("Media type filter")).toBeInTheDocument();
  });

  it("renders sort controls", () => {
    renderApp();
    expect(screen.getByLabelText("Sort field")).toBeInTheDocument();
  });

  it("renders the status bar", () => {
    const { container } = renderApp();
    expect(container.querySelector(".statusbar")).toBeInTheDocument();
  });

  it("renders the toast container", () => {
    const { container } = renderApp();
    expect(container.querySelector(".toast-container")).toBeInTheDocument();
  });

  it("renders main area", () => {
    const { container } = renderApp();
    expect(container.querySelector(".main-area")).toBeInTheDocument();
  });

  it("calls initApp via useAppInit on mount", async () => {
    renderApp();
    await waitFor(() => {
      expect(mockInitApp).toHaveBeenCalled();
    });
  });

  it("does not show setup modal when setup is null", () => {
    renderApp();
    expect(screen.queryByText(/Setup Required/i)).not.toBeInTheDocument();
  });

  it("does not show resume modal by default", () => {
    renderApp();
    expect(screen.queryByText(/Resume/i)).not.toBeInTheDocument();
  });

  it("does not show confirm delete root modal by default", () => {
    renderApp();
    expect(screen.queryByText(/Remove folder/i)).not.toBeInTheDocument();
  });

  it("does not show read-only banner by default", () => {
    const { container } = renderApp();
    expect(container.querySelector(".readonly-banner")).not.toBeInTheDocument();
  });

  it("sidebar is not collapsed by default", () => {
    const { container } = renderApp();
    expect(container.querySelector(".sidebar-collapsed")).not.toBeInTheDocument();
  });

  it("renders the sidebar collapse toggle", () => {
    renderApp();
    expect(screen.getByLabelText("Hide sidebar")).toBeInTheDocument();
  });
});

describe("App - sidebar toggle", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("toggles sidebar collapsed state on click", async () => {
    const user = userEvent.setup();
    renderApp();
    await user.click(screen.getByLabelText("Hide sidebar"));
    expect(screen.getByLabelText("Show sidebar")).toBeInTheDocument();
  });

  it("applies sidebar-collapsed class when sidebar is collapsed", async () => {
    const user = userEvent.setup();
    const { container } = renderApp();
    await user.click(screen.getByLabelText("Hide sidebar"));
    expect(container.querySelector(".sidebar-collapsed")).toBeInTheDocument();
  });
});

describe("App - close window", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("calls getCurrentWindow().close when close button clicked", async () => {
    const mockClose = vi.fn();
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    (getCurrentWindow as ReturnType<typeof vi.fn>).mockReturnValue({
      close: mockClose,
      minimize: vi.fn(),
      toggleMaximize: vi.fn(),
      destroy: vi.fn(),
    });
    const user = userEvent.setup();
    renderApp();
    await user.click(screen.getByLabelText("Close"));
    expect(mockClose).toHaveBeenCalled();
  });
});

describe("App - API integration on init", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockInitApp.mockResolvedValue(null);
  });

  it("calls getCliFolderPath during initApp", async () => {
    const spy = vi.spyOn(api, "getCliFolderPath").mockResolvedValue(null);
    mockInitApp.mockResolvedValueOnce({
      roots: [], scans: [], setupStatus: {
        isReady: true, ollamaAvailable: true, requiredModels: [], missingModels: [],
        instructions: [], download: { status: "idle", model: null, progressPct: 0, message: "" },
        pythonAvailable: true, pythonVersion: "3.12", suryaVenvOk: true,
        recommendedModel: "qwen2.5vl:7b", modelTier: "7b", modelSelectionReason: "",
        systemPythonFound: true,
        venvProvision: { status: "idle", step: "", progressPct: 0, message: "" },
        ffmpegAvailable: true,
      }, readOnly: false,
    });
    renderApp();
    await waitFor(() => {
      expect(spy).toHaveBeenCalled();
    });
    spy.mockRestore();
  });

  it("does not start scan when getCliFolderPath returns null", async () => {
    const cliSpy = vi.spyOn(api, "getCliFolderPath").mockResolvedValue(null);
    const startScanSpy = vi.spyOn(api, "startScan");
    mockInitApp.mockResolvedValueOnce({
      roots: [], scans: [], setupStatus: {
        isReady: true, ollamaAvailable: true, requiredModels: [], missingModels: [],
        instructions: [], download: { status: "idle", model: null, progressPct: 0, message: "" },
        pythonAvailable: true, pythonVersion: "3.12", suryaVenvOk: true,
        recommendedModel: "qwen2.5vl:7b", modelTier: "7b", modelSelectionReason: "",
        systemPythonFound: true,
        venvProvision: { status: "idle", step: "", progressPct: 0, message: "" },
        ffmpegAvailable: true,
      }, readOnly: false,
    });
    renderApp();
    await waitFor(() => {
      expect(cliSpy).toHaveBeenCalled();
    });
    expect(startScanSpy).not.toHaveBeenCalled();
    startScanSpy.mockRestore();
    cliSpy.mockRestore();
  });

  it("does not start scan when initApp returns null", async () => {
    const startScanSpy = vi.spyOn(api, "startScan");
    mockInitApp.mockResolvedValueOnce(null);
    renderApp();
    await waitFor(() => {
      expect(mockInitApp).toHaveBeenCalled();
    });
    expect(startScanSpy).not.toHaveBeenCalled();
    startScanSpy.mockRestore();
  });

  const readySetupStatus = () => ({
    isReady: true, ollamaAvailable: true, requiredModels: [], missingModels: [],
    instructions: [], download: { status: "idle", model: null, progressPct: 0, message: "" },
    pythonAvailable: true, pythonVersion: "3.12", suryaVenvOk: true,
    recommendedModel: "qwen2.5vl:7b", modelTier: "7b", modelSelectionReason: "",
    systemPythonFound: true,
    venvProvision: { status: "idle", step: "", progressPct: 0, message: "" },
    ffmpegAvailable: true,
  });

  it("starts scan when CLI path matches an existing root with interrupted scan", async () => {
    vi.spyOn(api, "getCliFolderPath").mockResolvedValue("/home/user/photos");
    const startScanSpy = vi.spyOn(api, "startScan").mockResolvedValue({
      id: 99, rootId: 1, rootPath: "/home/user/photos", status: "running",
      scanMarker: 0, totalFiles: 0, processedFiles: 0, progressPct: 0,
      added: 0, modified: 0, moved: 0, unchanged: 0, deleted: 0,
      startedAt: 0, updatedAt: 0, phase: "discovering" as const, discoveredFiles: 0,
    });
    mockInitApp.mockResolvedValueOnce({
      roots: [mockRoot],
      scans: [{
        id: 10, rootId: 1, rootPath: "/home/user/photos", status: "interrupted",
        scanMarker: 0, totalFiles: 10, processedFiles: 5, progressPct: 50,
        added: 5, modified: 0, moved: 0, unchanged: 0, deleted: 0,
        startedAt: 0, updatedAt: 0, phase: "processing" as const, discoveredFiles: 0,
      }],
      setupStatus: readySetupStatus(),
      readOnly: false,
    });
    renderApp();
    await waitFor(() => {
      expect(startScanSpy).toHaveBeenCalledWith("/home/user/photos");
    }, { timeout: 3000 });
    expect(mockAddTrackedJobId).toHaveBeenCalledWith(99);
    startScanSpy.mockRestore();
  });

  it("starts scan and refreshes roots when CLI path is a new root", async () => {
    vi.spyOn(api, "getCliFolderPath").mockResolvedValue("/home/user/new-path");
    vi.spyOn(api, "startScan").mockResolvedValue({
      id: 100, rootId: 2, rootPath: "/home/user/new-path", status: "running",
      scanMarker: 0, totalFiles: 0, processedFiles: 0, progressPct: 0,
      added: 0, modified: 0, moved: 0, unchanged: 0, deleted: 0,
      startedAt: 0, updatedAt: 0, phase: "discovering" as const, discoveredFiles: 0,
    });
    vi.spyOn(api, "listRoots").mockResolvedValue([
      { ...mockRoot, id: 2, rootPath: "/home/user/new-path", rootName: "new-path" },
    ]);
    mockInitApp.mockResolvedValueOnce({
      roots: [mockRoot],
      scans: [],
      setupStatus: readySetupStatus(),
      readOnly: false,
    });
    renderApp();
    await waitFor(() => {
      expect(api.startScan).toHaveBeenCalledWith("/home/user/new-path");
    }, { timeout: 3000 });
    await waitFor(() => {
      expect(mockRefreshRoots).toHaveBeenCalled();
    }, { timeout: 3000 });
  });

  it("does not start scan when a running scan already exists for new CLI path", async () => {
    vi.spyOn(api, "getCliFolderPath").mockResolvedValue("/home/user/new-path");
    const startScanSpy = vi.spyOn(api, "startScan");
    mockInitApp.mockResolvedValueOnce({
      roots: [mockRoot],
      scans: [{
        id: 10, rootId: 1, rootPath: "/other/path", status: "running",
        scanMarker: 0, totalFiles: 10, processedFiles: 5, progressPct: 50,
        added: 5, modified: 0, moved: 0, unchanged: 0, deleted: 0,
        startedAt: 0, updatedAt: 0, phase: "processing" as const, discoveredFiles: 0,
      }],
      setupStatus: readySetupStatus(),
      readOnly: false,
    });
    renderApp();
    await waitFor(() => {
      expect(mockInitApp).toHaveBeenCalled();
    });
    expect(startScanSpy).not.toHaveBeenCalled();
    startScanSpy.mockRestore();
  });

  it("does not start scan when readOnly is true", async () => {
    vi.spyOn(api, "getCliFolderPath").mockResolvedValue("/home/user/photos");
    const startScanSpy = vi.spyOn(api, "startScan");
    mockInitApp.mockResolvedValueOnce({
      roots: [mockRoot],
      scans: [{
        id: 10, rootId: 1, rootPath: "/home/user/photos", status: "interrupted",
        scanMarker: 0, totalFiles: 10, processedFiles: 5, progressPct: 50,
        added: 5, modified: 0, moved: 0, unchanged: 0, deleted: 0,
        startedAt: 0, updatedAt: 0, phase: "processing" as const, discoveredFiles: 0,
      }],
      setupStatus: readySetupStatus(),
      readOnly: true,
    });
    renderApp();
    await waitFor(() => {
      expect(mockInitApp).toHaveBeenCalled();
    });
    expect(startScanSpy).not.toHaveBeenCalled();
    startScanSpy.mockRestore();
  });

  it("does not start scan when setup is not ready", async () => {
    vi.spyOn(api, "getCliFolderPath").mockResolvedValue("/home/user/photos");
    const startScanSpy = vi.spyOn(api, "startScan");
    mockInitApp.mockResolvedValueOnce({
      roots: [mockRoot],
      scans: [],
      setupStatus: {
        ...readySetupStatus(),
        isReady: false,
        ollamaAvailable: false,
        missingModels: ["qwen2.5vl:7b"],
      },
      readOnly: false,
    });
    renderApp();
    await waitFor(() => {
      expect(mockInitApp).toHaveBeenCalled();
    });
    expect(startScanSpy).not.toHaveBeenCalled();
    startScanSpy.mockRestore();
  });
});

describe("App - default view mode", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("renders Content view by default with toolbar, not Faces/Duplicates/PDF Passwords views", () => {
    renderApp();
    expect(screen.getByLabelText("Search query")).toBeInTheDocument();
    expect(screen.getByLabelText("Media type filter")).toBeInTheDocument();
  });

  it("shows Find Duplicates, Faces, and PDF Passwords tool buttons in sidebar", () => {
    renderApp();
    expect(screen.getByText("Find Duplicates")).toBeInTheDocument();
    expect(screen.getByText("Faces")).toBeInTheDocument();
    expect(screen.getByText("PDF Passwords")).toBeInTheDocument();
  });
});
