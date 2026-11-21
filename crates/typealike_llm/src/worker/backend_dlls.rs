//! The DLLs of the llama.cpp backends that the worker loads. A Windows GPU build has a folder of
//! DLLs for each backend next to the program, and the worker loads only the CPU backend and the
//! GPU backend that a load names. Thus a backend that no load uses takes no memory.

use std::path::{Path, PathBuf};

use typealike_core::inference::Backend;

/// The folder next to the program that has a folder of DLLs for each llama.cpp backend. The build
/// script of this crate writes the same folders.
const BACKENDS_FOLDER: &str = "backends";

/// A folder of DLLs of one llama.cpp backend, in [`BACKENDS_FOLDER`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BackendFolder {
    /// The CPU backend, with a DLL for each kind of CPU. llama.cpp loads the best DLL for the CPU.
    Cpu,
    Vulkan,
    Cuda,
}

impl BackendFolder {
    /// The name of the folder: the name of the backend in the names of its DLLs.
    fn name(self) -> &'static str {
        match self {
            Self::Cpu => "cpu",
            Self::Vulkan => "vulkan",
            Self::Cuda => "cuda",
        }
    }

    /// Returns the folders whose backends a load on `backend` needs. Each model keeps some parts
    /// on the CPU, so a GPU load also needs the CPU backend. Metal is in the program.
    fn needed_by(backend: Backend) -> &'static [Self] {
        match backend {
            Backend::Cpu => &[Self::Cpu],
            Backend::Vulkan => &[Self::Cpu, Self::Vulkan],
            Backend::Cuda => &[Self::Cpu, Self::Cuda],
            Backend::Metal => &[],
        }
    }
}

/// Where the worker finds the DLLs of the llama.cpp backends, and which of them loaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct BackendDlls {
    /// The folder next to the program with a folder for each backend. With `None`, llama.cpp finds
    /// the DLLs when it starts: in the folder of its build, next to the program and in the current
    /// folder. llama.cpp then loads each backend that it finds.
    root: Option<PathBuf>,
    /// The folders that loaded before.
    loaded: Vec<BackendFolder>,
}

impl BackendDlls {
    /// Finds the backend DLLs of the program in `program_folder`: in the backend folders when
    /// `is_folder` finds the folder of the CPU backend there. Else llama.cpp finds them, as for the
    /// tests, the examples and the builds without backend folders.
    pub(super) fn of_program_in(program_folder: Option<&Path>, is_folder: impl Fn(&Path) -> bool) -> Self {
        let root = program_folder
            .map(|folder| folder.join(BACKENDS_FOLDER))
            .filter(|root| is_folder(&root.join(BackendFolder::Cpu.name())));
        Self {
            root,
            loaded: Vec::new(),
        }
    }

    /// The DLLs that llama.cpp finds when it starts, as for a program without backend folders.
    #[cfg(test)]
    pub(super) fn found_by_llama_cpp() -> Self {
        Self {
            root: None,
            loaded: Vec::new(),
        }
    }

    /// Returns the folders of the backends that a load on `backend` needs and that did not load
    /// before, and marks them as loaded. Returns no folder when llama.cpp finds the DLLs.
    pub(super) fn folders_to_load(&mut self, backend: Backend) -> Vec<PathBuf> {
        let Some(root) = &self.root else {
            return Vec::new();
        };
        let new_folders: Vec<BackendFolder> = BackendFolder::needed_by(backend)
            .iter()
            .copied()
            .filter(|folder| !self.loaded.contains(folder))
            .collect();
        self.loaded.extend(&new_folders);
        new_folders
            .into_iter()
            .map(|folder| root.join(folder.name()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROGRAM_FOLDER: &str = "app";

    fn program_with_backend_folders() -> BackendDlls {
        BackendDlls::of_program_in(Some(Path::new(PROGRAM_FOLDER)), |_| true)
    }

    fn backend_folder(name: &str) -> PathBuf {
        Path::new(PROGRAM_FOLDER).join("backends").join(name)
    }

    #[test]
    fn each_backend_loads_the_cpu_folder_and_only_its_own_gpu_folder() {
        let cases = [
            (Backend::Cpu, vec![backend_folder("cpu")]),
            (
                Backend::Vulkan,
                vec![backend_folder("cpu"), backend_folder("vulkan")],
            ),
            (Backend::Cuda, vec![backend_folder("cpu"), backend_folder("cuda")]),
            (Backend::Metal, vec![]),
        ];

        let folders = cases
            .clone()
            .map(|(backend, _)| program_with_backend_folders().folders_to_load(backend));

        assert_eq!(folders, cases.map(|(_, expected)| expected));
    }

    #[test]
    fn another_gpu_backend_loads_only_its_own_folder() {
        let mut dlls = program_with_backend_folders();
        dlls.folders_to_load(Backend::Vulkan);

        let folders = dlls.folders_to_load(Backend::Cuda);

        assert_eq!(folders, [backend_folder("cuda")]);
    }

    #[test]
    fn second_load_on_the_same_backend_loads_no_folder() {
        let mut dlls = program_with_backend_folders();
        dlls.folders_to_load(Backend::Vulkan);

        let folders = dlls.folders_to_load(Backend::Vulkan);

        assert_eq!(folders, Vec::<PathBuf>::new());
    }

    #[test]
    fn program_without_the_cpu_folder_leaves_the_dlls_to_llama_cpp() {
        let only_gpu_folders = |folder: &Path| !folder.ends_with("cpu");

        let dlls = BackendDlls::of_program_in(Some(Path::new(PROGRAM_FOLDER)), only_gpu_folders);

        assert_eq!(dlls, BackendDlls::found_by_llama_cpp());
    }

    #[test]
    fn unknown_program_folder_leaves_the_dlls_to_llama_cpp() {
        let dlls = BackendDlls::of_program_in(None, |_| true);

        assert_eq!(dlls, BackendDlls::found_by_llama_cpp());
    }

    #[test]
    fn llama_cpp_that_finds_the_dlls_loads_no_folder() {
        let mut dlls = BackendDlls::found_by_llama_cpp();

        let folders = dlls.folders_to_load(Backend::Cuda);

        assert_eq!(folders, Vec::<PathBuf>::new());
    }
}
