import { useEffect, useState, useRef } from "react";
import { useNavigate } from "react-router-dom";
import { Plus, Trash2, Play, Clock, AlertTriangle, LogOut, Download, Upload } from "lucide-react";
import { useFlowListStore } from "@/stores/flowListStore";
import { useAuthStore } from "@/stores/authStore";
import { flowsApi } from "@/api/flows";

export function FlowListPage() {
  const { flows, loading, fetchFlows, createFlow, deleteFlow } =
    useFlowListStore();
  const { logout, user } = useAuthStore();
  const navigate = useNavigate();
  const [showCreate, setShowCreate] = useState(false);
  const [newName, setNewName] = useState("");
  const [deleteTarget, setDeleteTarget] = useState<{ id: string; name: string } | null>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    fetchFlows();
  }, [fetchFlows]);

  const handleCreate = async () => {
    if (!newName.trim()) return;
    const id = await createFlow(newName.trim());
    setShowCreate(false);
    setNewName("");
    navigate(`/flow/${id}`);
  };

  const handleExport = async (e: React.MouseEvent, flowId: string, flowName: string) => {
    e.stopPropagation();
    try {
      const data = await flowsApi.export(flowId);
      const blob = new Blob([JSON.stringify(data, null, 2)], { type: "application/json" });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = `${flowName.replace(/[^a-zA-Z0-9_-]/g, "_")}.z8flow.json`;
      a.click();
      URL.revokeObjectURL(url);
    } catch (err) {
      console.error("Export failed:", err);
    }
  };

  const handleImport = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;
    try {
      const text = await file.text();
      const data = JSON.parse(text);
      if (!data.flow) {
        alert("Invalid z8run export file");
        return;
      }
      const result = await flowsApi.import(data);
      await fetchFlows();
      navigate(`/flow/${result.id}`);
    } catch (err) {
      console.error("Import failed:", err);
      alert("Failed to import flow. Make sure the file is a valid z8run export.");
    }
    // Reset input so the same file can be re-imported
    if (fileInputRef.current) fileInputRef.current.value = "";
  };

  return (
    <div className="min-h-screen bg-slate-950">
      {/* Hidden file input for import */}
      <input
        ref={fileInputRef}
        type="file"
        accept=".json,.z8flow.json"
        className="hidden"
        onChange={handleImport}
      />

      {/* Top bar */}
      <div className="border-b border-slate-800">
        <div className="max-w-5xl mx-auto px-6 py-4 flex items-center justify-between">
          <div className="flex items-center gap-3">
            <div className="w-8 h-8 rounded-lg bg-z8-600 flex items-center justify-center">
              <span className="text-xs font-bold text-white">z8</span>
            </div>
            <div>
              <h1 className="text-lg font-semibold text-slate-100">z8run</h1>
              <p className="text-xs text-slate-500">Flow Engine</p>
            </div>
          </div>
          <div className="flex items-center gap-3">
            {user && (
              <span className="text-xs text-slate-500">{user.email}</span>
            )}
            <button
              type="button"
              onClick={logout}
              className="p-2 text-slate-400 hover:text-slate-200 hover:bg-slate-800 rounded-lg transition-colors"
              title="Sign out"
            >
              <LogOut size={16} />
            </button>
            <button
              type="button"
              onClick={() => fileInputRef.current?.click()}
              className="flex items-center gap-2 px-3 py-2 text-slate-300 hover:bg-slate-800
                text-sm rounded-lg transition-colors border border-slate-700"
              title="Import flow from JSON"
            >
              <Upload size={14} />
              Import
            </button>
            <button
              type="button"
              onClick={() => setShowCreate(true)}
              className="flex items-center gap-2 px-4 py-2 bg-z8-600 hover:bg-z8-700
                text-white text-sm font-medium rounded-lg transition-colors"
            >
              <Plus size={16} />
              New Flow
            </button>
          </div>
        </div>
      </div>

      {/* Content */}
      <div className="max-w-5xl mx-auto px-6 py-8">
        {/* Create dialog */}
        {showCreate && (
          <div className="mb-6 bg-slate-900 border border-slate-700 rounded-lg p-4">
            <h3 className="text-sm font-medium text-slate-200 mb-3">
              Create New Flow
            </h3>
            <div className="flex gap-3">
              <input
                type="text"
                value={newName}
                onChange={(e) => setNewName(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && handleCreate()}
                placeholder="Flow name..."
                className="flex-1 bg-slate-800 border border-slate-700 rounded-md px-3 py-2
                  text-sm text-slate-200 placeholder-slate-500 focus:outline-none
                  focus:border-z8-500"
                autoFocus
              />
              <button
                type="button"
                onClick={handleCreate}
                className="px-4 py-2 bg-z8-600 hover:bg-z8-700 text-white text-sm
                  rounded-md transition-colors"
              >
                Create
              </button>
              <button
                type="button"
                onClick={() => setShowCreate(false)}
                className="px-4 py-2 text-slate-400 hover:bg-slate-800 text-sm
                  rounded-md transition-colors"
              >
                Cancel
              </button>
            </div>
          </div>
        )}

        {/* Flow list */}
        {loading ? (
          <div className="text-center text-slate-500 py-20">
            Loading flows...
          </div>
        ) : flows.length === 0 ? (
          <div className="text-center py-20">
            <div className="w-16 h-16 rounded-full bg-slate-800 flex items-center justify-center mx-auto mb-4">
              <Play size={24} className="text-slate-600" />
            </div>
            <h2 className="text-lg font-medium text-slate-300 mb-2">
              No flows yet
            </h2>
            <p className="text-sm text-slate-500 mb-4">
              Create your first flow or import one
            </p>
            <div className="flex items-center justify-center gap-3">
              <button
                type="button"
                onClick={() => setShowCreate(true)}
                className="px-4 py-2 bg-z8-600 hover:bg-z8-700 text-white text-sm
                  font-medium rounded-lg transition-colors"
              >
                Create Flow
              </button>
              <button
                type="button"
                onClick={() => fileInputRef.current?.click()}
                className="flex items-center gap-2 px-4 py-2 text-slate-300 hover:bg-slate-800
                  text-sm rounded-lg transition-colors border border-slate-700"
              >
                <Upload size={14} />
                Import
              </button>
            </div>
          </div>
        ) : (
          <div className="grid gap-3">
            {flows.map((flow) => (
              <div
                key={flow.id}
                onClick={() => navigate(`/flow/${flow.id}`)}
                onKeyDown={(e) =>
                  e.key === "Enter" && navigate(`/flow/${flow.id}`)
                }
                role="button"
                tabIndex={0}
                className="bg-slate-900 border border-slate-800 hover:border-slate-700
                  rounded-lg p-4 cursor-pointer transition-colors group"
              >
                <div className="flex items-center justify-between">
                  <div>
                    <h3 className="text-sm font-medium text-slate-200 group-hover:text-white">
                      {flow.name}
                    </h3>
                    <div className="flex items-center gap-4 mt-1">
                      <span className="text-xs text-slate-500">
                        {flow.nodes} nodes &middot; {flow.edges} edges
                      </span>
                      <span className="flex items-center gap-1 text-xs text-slate-600">
                        <Clock size={10} />
                        {new Date(flow.updated_at).toLocaleDateString()}
                      </span>
                    </div>
                  </div>
                  <div className="flex items-center gap-2">
                    <span
                      className={`text-[10px] font-medium px-2 py-0.5 rounded-full ${
                        flow.status === "running"
                          ? "bg-green-900/50 text-green-400"
                          : "bg-slate-800 text-slate-500"
                      }`}
                    >
                      {flow.status}
                    </span>
                    <button
                      type="button"
                      onClick={(e) => handleExport(e, flow.id, flow.name)}
                      className="p-1.5 hover:bg-slate-800 rounded transition-colors opacity-0
                        group-hover:opacity-100"
                      title="Export flow"
                    >
                      <Download size={14} className="text-slate-400" />
                    </button>
                    <button
                      type="button"
                      onClick={(e) => {
                        e.stopPropagation();
                        setDeleteTarget({ id: flow.id, name: flow.name });
                      }}
                      className="p-1.5 hover:bg-red-900/30 rounded transition-colors opacity-0
                        group-hover:opacity-100"
                    >
                      <Trash2 size={14} className="text-red-400" />
                    </button>
                  </div>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* Delete confirmation modal */}
      {deleteTarget && (
        <div className="fixed inset-0 bg-black/60 flex items-center justify-center z-50">
          <div className="bg-slate-900 border border-slate-700 rounded-lg p-6 max-w-sm w-full mx-4 shadow-xl">
            <div className="flex items-center gap-3 mb-4">
              <div className="w-10 h-10 rounded-full bg-red-900/30 flex items-center justify-center">
                <AlertTriangle size={20} className="text-red-400" />
              </div>
              <div>
                <h3 className="text-sm font-semibold text-slate-200">Delete Flow</h3>
                <p className="text-xs text-slate-400 mt-0.5">This action cannot be undone</p>
              </div>
            </div>
            <p className="text-sm text-slate-300 mb-6">
              Are you sure you want to delete <span className="font-medium text-white">"{deleteTarget.name}"</span>?
            </p>
            <div className="flex justify-end gap-3">
              <button
                type="button"
                onClick={() => setDeleteTarget(null)}
                className="px-4 py-2 text-sm text-slate-400 hover:bg-slate-800 rounded-md transition-colors"
              >
                Cancel
              </button>
              <button
                type="button"
                onClick={() => {
                  deleteFlow(deleteTarget.id);
                  setDeleteTarget(null);
                }}
                className="px-4 py-2 text-sm bg-red-600 hover:bg-red-700 text-white rounded-md transition-colors"
              >
                Delete
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
