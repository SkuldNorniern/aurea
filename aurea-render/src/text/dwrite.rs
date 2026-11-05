//! The parts of DirectWrite text needs, over its COM interfaces.

use std::collections::HashMap;
use std::ffi::OsString;
use std::iter::once;
use std::mem::zeroed;
use std::ops::Deref;
use std::os::windows::ffi::OsStringExt;
use std::path::PathBuf;
use std::ptr::{NonNull, from_ref, null, null_mut, read_unaligned};
use std::slice::from_raw_parts;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{LazyLock, Mutex, OnceLock};

use aurea_foundation::lock;
use winapi::Interface;
use winapi::ctypes::c_void;
use winapi::shared::guiddef::{IsEqualGUID, REFIID};
use winapi::shared::minwindef::{BOOL, ULONG};
use winapi::shared::windef::RECT;
use winapi::shared::winerror::{E_FAIL, E_NOINTERFACE, HRESULT, S_OK, SUCCEEDED};
use winapi::um::dcommon::DWRITE_MEASURING_MODE_NATURAL;
use winapi::um::dwrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_METRICS, DWRITE_FONT_SIMULATIONS_NONE,
    DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_ITALIC, DWRITE_FONT_STYLE_NORMAL,
    DWRITE_GLYPH_RUN, DWRITE_RENDERING_MODE_NATURAL, DWRITE_TEXTURE_CLEARTYPE_3x1,
    DWriteCreateFactory, IDWriteFactory, IDWriteFontCollection, IDWriteFontFace, IDWriteFontFamily,
    IDWriteFontFile, IDWriteFontFileLoader, IDWriteFontFileLoaderVtbl, IDWriteFontFileStream,
    IDWriteFontFileStreamVtbl, IDWriteLocalFontFileLoader,
};
use winapi::um::dwrite_3::{DWRITE_FONT_AXIS_VALUE, IDWriteFontFace5};
use winapi::um::unknwnbase::{IUnknown, IUnknownVtbl};
use winapi::um::winnt::WCHAR;

use crate::text::glyph::FontBytes;
use crate::text::platform::SubpixelGlyph;
use crate::types::FontVariation;

/// An owned reference to a COM object.
struct Com<T>(NonNull<T>);

// SAFETY: every object here comes from a shared factory, and those are free-threaded.
unsafe impl<T> Send for Com<T> {}
// SAFETY: as above.
unsafe impl<T> Sync for Com<T> {}

impl<T> Com<T> {
    /// The object `call` writes out, when it succeeds.
    fn created(call: impl FnOnce(*mut *mut T) -> HRESULT) -> Option<Self> {
        let mut out = null_mut();
        if !SUCCEEDED(call(&raw mut out)) {
            return None;
        }
        NonNull::new(out).map(Self)
    }

    fn as_ptr(&self) -> *mut T {
        self.0.as_ptr()
    }

    fn unknown(&self) -> &IUnknown {
        // SAFETY: every COM interface starts with IUnknown's.
        unsafe { self.0.cast().as_ref() }
    }

    /// The same object as another of its interfaces, when it has it.
    fn cast<U: Interface>(&self) -> Option<Com<U>> {
        Com::created(|out: *mut *mut U| {
            // SAFETY: `out` receives a reference of the asked type.
            unsafe { self.unknown().QueryInterface(&U::uuidof(), out.cast()) }
        })
    }
}

impl<T> Deref for Com<T> {
    type Target = T;

    fn deref(&self) -> &T {
        // SAFETY: the reference held keeps the object alive.
        unsafe { self.0.as_ref() }
    }
}

impl<T> Clone for Com<T> {
    fn clone(&self) -> Self {
        // SAFETY: the object is alive; this adds the reference the clone releases.
        unsafe { self.unknown().AddRef() };
        Self(self.0)
    }
}

impl<T> Drop for Com<T> {
    fn drop(&mut self) {
        // SAFETY: gives back the reference this holds.
        unsafe { self.unknown().Release() };
    }
}

fn factory() -> Option<&'static Com<IDWriteFactory>> {
    static FACTORY: OnceLock<Option<Com<IDWriteFactory>>> = OnceLock::new();
    FACTORY
        .get_or_init(|| {
            Com::created(|out: *mut *mut IDWriteFactory| {
                // SAFETY: `out` receives a reference of the asked type.
                unsafe {
                    DWriteCreateFactory(
                        DWRITE_FACTORY_TYPE_SHARED,
                        &IDWriteFactory::uuidof(),
                        out.cast(),
                    )
                }
            })
        })
        .as_ref()
}

fn wide(text: &str) -> Vec<WCHAR> {
    text.encode_utf16().chain(once(0)).collect()
}

/// The fonts installed on the system.
pub(super) struct FontCollection(Com<IDWriteFontCollection>);

impl FontCollection {
    pub(super) fn system() -> Option<Self> {
        let factory = factory()?;
        // SAFETY: `out` receives a collection reference.
        Com::created(|out| unsafe { factory.GetSystemFontCollection(out, 0) }).map(Self)
    }

    pub(super) fn family_by_name(&self, name: &str) -> Option<FontFamily> {
        let name = wide(name);
        let (mut index, mut exists): (u32, BOOL) = (0, 0);
        // SAFETY: `name` ends in a NUL and the outputs are valid.
        let found = unsafe {
            self.0
                .FindFamilyName(name.as_ptr(), &raw mut index, &raw mut exists)
        };
        if !SUCCEEDED(found) || exists == 0 {
            return None;
        }
        self.family(index)
    }

    fn family(&self, index: u32) -> Option<FontFamily> {
        // SAFETY: `out` receives a family reference.
        Com::created(|out| unsafe { self.0.GetFontFamily(index, out) }).map(FontFamily)
    }

    /// Every family's name, in English when it has one.
    pub(super) fn family_names(&self) -> Vec<String> {
        // SAFETY: takes no arguments.
        let count = unsafe { self.0.GetFontFamilyCount() };
        let mut names: Vec<String> = (0..count)
            .filter_map(|index| self.family(index)?.name())
            .collect();
        names.sort_unstable();
        names.dedup();
        names
    }
}

/// A family of installed fonts.
pub(super) struct FontFamily(Com<IDWriteFontFamily>);

impl FontFamily {
    /// The closest face to a weight and style, at normal width.
    pub(super) fn matching_face(&self, weight: u16, italic: bool) -> Option<FontFace> {
        let style = if italic {
            DWRITE_FONT_STYLE_ITALIC
        } else {
            DWRITE_FONT_STYLE_NORMAL
        };
        let font = Com::created(|out| {
            // SAFETY: `out` receives a font reference.
            unsafe {
                self.0.GetFirstMatchingFont(
                    u32::from(weight),
                    DWRITE_FONT_STRETCH_NORMAL,
                    style,
                    out,
                )
            }
        })?;
        // SAFETY: `out` receives a face reference.
        Com::created(|out| unsafe { font.CreateFontFace(out) }).map(FontFace)
    }

    fn name(&self) -> Option<String> {
        // SAFETY: `out` receives a strings reference.
        let names = Com::created(|out| unsafe { self.0.GetFamilyNames(out) })?;
        let locale = wide("en-us");
        let (mut index, mut exists): (u32, BOOL) = (0, 0);
        // SAFETY: `locale` ends in a NUL and the outputs are valid.
        let found =
            unsafe { names.FindLocaleName(locale.as_ptr(), &raw mut index, &raw mut exists) };
        if !SUCCEEDED(found) || exists == 0 {
            index = 0;
        }
        let mut length = 0;
        // SAFETY: the output is valid.
        if !SUCCEEDED(unsafe { names.GetStringLength(index, &raw mut length) }) {
            return None;
        }
        let mut name: Vec<WCHAR> = vec![0; length as usize + 1];
        // SAFETY: the buffer holds `length` characters and their NUL.
        let copied = unsafe { names.GetString(index, name.as_mut_ptr(), length + 1) };
        name.pop();
        SUCCEEDED(copied).then(|| String::from_utf16_lossy(&name))
    }
}

/// One font face.
#[derive(Clone)]
pub(super) struct FontFace(Com<IDWriteFontFace>);

impl FontFace {
    /// The face at `index` of a font file held in memory.
    pub(super) fn from_bytes(bytes: FontBytes, index: u32) -> Option<Self> {
        let factory = factory()?;
        let file = memory_file(factory, bytes)?;
        let (mut supported, mut file_type, mut face_type, mut faces): (BOOL, u32, u32, u32) =
            (0, 0, 0, 0);
        // SAFETY: the outputs are valid.
        let analyzed = unsafe {
            file.Analyze(
                &raw mut supported,
                &raw mut file_type,
                &raw mut face_type,
                &raw mut faces,
            )
        };
        if !SUCCEEDED(analyzed) || supported == 0 {
            return None;
        }
        let files = [file.as_ptr()];
        Com::created(|out| {
            // SAFETY: `files` holds the one file it says, alive through the call.
            unsafe {
                factory.CreateFontFace(
                    face_type,
                    1,
                    files.as_ptr(),
                    index,
                    DWRITE_FONT_SIMULATIONS_NONE,
                    out,
                )
            }
        })
        .map(Self)
    }

    /// This face at `variations`, when it has variable axes and the system can vary them.
    pub(super) fn varied(&self, variations: &[FontVariation]) -> Option<Self> {
        let face = self.0.cast::<IDWriteFontFace5>()?;
        // SAFETY: takes no arguments.
        let count = unsafe { face.GetFontAxisValueCount() };
        if count == 0 {
            return None;
        }
        // SAFETY: an axis value is plain integers and floats.
        let mut axes: Vec<DWRITE_FONT_AXIS_VALUE> = vec![unsafe { zeroed() }; count as usize];
        // SAFETY: `axes` holds `count` values.
        if !SUCCEEDED(unsafe { face.GetFontAxisValues(axes.as_mut_ptr(), count) }) {
            return None;
        }
        for variation in variations {
            let tag = u32::from_le_bytes(variation.tag);
            match axes.iter_mut().find(|axis| axis.axisTag == tag) {
                Some(axis) => axis.value = variation.value,
                None => axes.push(DWRITE_FONT_AXIS_VALUE {
                    axisTag: tag,
                    value: variation.value,
                }),
            }
        }
        // SAFETY: `out` receives a resource reference.
        let resource = Com::created(|out| unsafe { face.GetFontResource(out) })?;
        let count = u32::try_from(axes.len()).ok()?;
        let varied = Com::created(|out| {
            // SAFETY: `axes` holds `count` values and `out` receives a face reference.
            unsafe {
                resource.CreateFontFace(DWRITE_FONT_SIMULATIONS_NONE, axes.as_ptr(), count, out)
            }
        })?;
        varied.cast().map(Self)
    }

    /// Ascent, descent and units per em.
    pub(super) fn metrics(&self) -> (f32, f32, f32) {
        // SAFETY: the metrics are plain integers.
        let mut metrics: DWRITE_FONT_METRICS = unsafe { zeroed() };
        // SAFETY: the output is valid.
        unsafe { self.0.GetMetrics(&raw mut metrics) };
        (
            f32::from(metrics.ascent),
            f32::from(metrics.descent),
            f32::from(metrics.designUnitsPerEm.max(1)),
        )
    }

    /// The face's glyph for `ch`, when it has one.
    pub(super) fn glyph_index(&self, ch: char) -> Option<u16> {
        let code_point = u32::from(ch);
        let mut glyph = 0;
        // SAFETY: one code point in, one glyph out.
        let found = unsafe {
            self.0
                .GetGlyphIndices(&raw const code_point, 1, &raw mut glyph)
        };
        (SUCCEEDED(found) && glyph != 0).then_some(glyph)
    }

    /// Which face of its font file this is.
    pub(super) fn index(&self) -> u32 {
        // SAFETY: takes no arguments.
        unsafe { self.0.GetIndex() }
    }

    /// The font file the face comes from.
    pub(super) fn file(&self) -> Option<FontFile> {
        let mut count = 1;
        let mut file = null_mut();
        // SAFETY: there is room for the one file; a face made of more fails here and has no file.
        let found = unsafe { self.0.GetFiles(&raw mut count, &raw mut file) };
        if !SUCCEEDED(found) {
            return None;
        }
        NonNull::new(file).map(|file| FontFile(Com(file)))
    }

    /// `glyph` at `size` as hinted ClearType coverage.
    pub(super) fn subpixel_glyph(&self, glyph: u16, size: f32) -> Option<SubpixelGlyph> {
        let factory = factory()?;
        let glyphs = [glyph];
        let run = DWRITE_GLYPH_RUN {
            fontFace: self.0.as_ptr(),
            fontEmSize: size,
            glyphCount: 1,
            glyphIndices: glyphs.as_ptr(),
            glyphAdvances: null(),
            glyphOffsets: null(),
            isSideways: 0,
            bidiLevel: 0,
        };
        let analysis = Com::created(|out| {
            // SAFETY: `run` and the glyphs it points at outlive the call.
            unsafe {
                factory.CreateGlyphRunAnalysis(
                    &raw const run,
                    1.0,
                    null(),
                    DWRITE_RENDERING_MODE_NATURAL,
                    DWRITE_MEASURING_MODE_NATURAL,
                    0.0,
                    0.0,
                    out,
                )
            }
        })?;
        // SAFETY: the bounds are plain integers.
        let mut bounds: RECT = unsafe { zeroed() };
        // SAFETY: the output is valid.
        let measured = unsafe {
            analysis.GetAlphaTextureBounds(DWRITE_TEXTURE_CLEARTYPE_3x1, &raw mut bounds)
        };
        if !SUCCEEDED(measured) {
            return None;
        }
        let width = u32::try_from(bounds.right - bounds.left).unwrap_or(0);
        let height = u32::try_from(bounds.bottom - bounds.top).unwrap_or(0);
        if width == 0 || height == 0 {
            return Some(SubpixelGlyph {
                width: 0,
                height: 0,
                left: 0,
                top: 0,
                coverage: Vec::new(),
            });
        }
        let size = width.checked_mul(height)?.checked_mul(3)?;
        let mut coverage = vec![0; size as usize];
        // SAFETY: `coverage` holds the `size` bytes these bounds take in this texture type.
        let drawn = unsafe {
            analysis.CreateAlphaTexture(
                DWRITE_TEXTURE_CLEARTYPE_3x1,
                &raw const bounds,
                coverage.as_mut_ptr(),
                size,
            )
        };
        SUCCEEDED(drawn).then_some(SubpixelGlyph {
            width,
            height,
            left: bounds.left,
            top: bounds.top,
            coverage,
        })
    }
}

/// A font file DirectWrite knows.
pub(super) struct FontFile(Com<IDWriteFontFile>);

impl FontFile {
    fn key(&self) -> Option<(*const c_void, u32)> {
        let (mut key, mut size) = (null(), 0);
        // SAFETY: the outputs are valid; the key lives as long as the file.
        let found = unsafe { self.0.GetReferenceKey(&raw mut key, &raw mut size) };
        SUCCEEDED(found).then_some((key, size))
    }

    fn loader(&self) -> Option<Com<IDWriteFontFileLoader>> {
        // SAFETY: `out` receives a loader reference.
        Com::created(|out| unsafe { self.0.GetLoader(out) })
    }

    /// Where the file is on disk, when it is on disk.
    pub(super) fn path(&self) -> Option<PathBuf> {
        let (key, key_size) = self.key()?;
        let loader = self.loader()?.cast::<IDWriteLocalFontFileLoader>()?;
        let mut length = 0;
        // SAFETY: the key belongs to this loader and the output is valid.
        let measured = unsafe { loader.GetFilePathLengthFromKey(key, key_size, &raw mut length) };
        if !SUCCEEDED(measured) {
            return None;
        }
        let mut path: Vec<WCHAR> = vec![0; length as usize + 1];
        // SAFETY: the buffer holds `length` characters and their NUL.
        let copied =
            unsafe { loader.GetFilePathFromKey(key, key_size, path.as_mut_ptr(), length + 1) };
        path.pop();
        SUCCEEDED(copied).then(|| OsString::from_wide(&path).into())
    }

    /// The whole file.
    pub(super) fn bytes(&self) -> Option<Vec<u8>> {
        let (key, key_size) = self.key()?;
        let loader = self.loader()?;
        // SAFETY: the key belongs to this loader and `out` receives a stream reference.
        let stream = Com::created(|out| unsafe { loader.CreateStreamFromKey(key, key_size, out) })?;
        let mut size = 0;
        // SAFETY: the output is valid.
        if !SUCCEEDED(unsafe { stream.GetFileSize(&raw mut size) }) {
            return None;
        }
        let (mut start, mut context) = (null(), null_mut());
        // SAFETY: the file holds this range and the outputs are valid.
        let read = unsafe { stream.ReadFileFragment(&raw mut start, 0, size, &raw mut context) };
        if !SUCCEEDED(read) {
            return None;
        }
        // SAFETY: the fragment holds `size` bytes until released below.
        let bytes = unsafe { from_raw_parts(start.cast::<u8>(), usize::try_from(size).ok()?) };
        let bytes = bytes.to_vec();
        // SAFETY: releases the fragment read above, once.
        unsafe { stream.ReleaseFileFragment(context) };
        Some(bytes)
    }
}

/// Fonts held in memory, by the key DirectWrite asks for them under. The fonts that come here are
/// registered ones, which stay until the app exits, so this does not grow past them.
static MEMORY_FONTS: LazyLock<Mutex<HashMap<u64, FontBytes>>> = LazyLock::new(Mutex::default);

/// The size of those keys.
const KEY_SIZE: u32 = u64::BITS / 8;

/// A DirectWrite font file reading `bytes`.
fn memory_file(factory: &Com<IDWriteFactory>, bytes: FontBytes) -> Option<Com<IDWriteFontFile>> {
    static LOADER_ADDED: OnceLock<bool> = OnceLock::new();
    let loader = from_ref(&MEMORY_LOADER).cast_mut().cast();
    let added = *LOADER_ADDED.get_or_init(|| {
        // SAFETY: the loader is static, so it outlives the factory's use of it.
        SUCCEEDED(unsafe { factory.RegisterFontFileLoader(loader) })
    });
    if !added {
        return None;
    }
    let contents: &[u8] = (*bytes).as_ref();
    let key = contents.as_ptr() as u64;
    lock(&MEMORY_FONTS).insert(key, bytes);
    Com::created(|out| {
        // SAFETY: the key is copied by the call and the loader is registered.
        unsafe {
            factory.CreateCustomFontFileReference((&raw const key).cast(), KEY_SIZE, loader, out)
        }
    })
}

/// The loader for [`MEMORY_FONTS`], a COM object with static lifetime.
#[repr(C)]
struct MemoryLoader(IDWriteFontFileLoader);

// SAFETY: it holds a pointer to a static table, and nothing writes through it.
unsafe impl Sync for MemoryLoader {}

static MEMORY_LOADER: MemoryLoader = MemoryLoader(IDWriteFontFileLoader {
    lpVtbl: &LOADER_TABLE,
});

static LOADER_TABLE: IDWriteFontFileLoaderVtbl = IDWriteFontFileLoaderVtbl {
    parent: IUnknownVtbl {
        QueryInterface: loader_query,
        AddRef: static_reference,
        Release: static_reference,
    },
    CreateStreamFromKey: loader_open,
};

unsafe extern "system" fn static_reference(_: *mut IUnknown) -> ULONG {
    1
}

unsafe extern "system" fn loader_query(
    this: *mut IUnknown,
    id: REFIID,
    out: *mut *mut c_void,
) -> HRESULT {
    // SAFETY: COM passes a valid id and output.
    unsafe {
        let known = IsEqualGUID(&*id, &IUnknown::uuidof())
            || IsEqualGUID(&*id, &IDWriteFontFileLoader::uuidof());
        *out = if known { this.cast() } else { null_mut() };
        if known { S_OK } else { E_NOINTERFACE }
    }
}

unsafe extern "system" fn loader_open(
    _: *mut IDWriteFontFileLoader,
    key: *const c_void,
    key_size: u32,
    out: *mut *mut IDWriteFontFileStream,
) -> HRESULT {
    if key_size != KEY_SIZE {
        return E_FAIL;
    }
    // SAFETY: the key is the `u64` `memory_file` gave, at any alignment.
    let key = unsafe { read_unaligned(key.cast::<u64>()) };
    let Some(bytes) = lock(&MEMORY_FONTS).get(&key).cloned() else {
        return E_FAIL;
    };
    let stream = Box::new(MemoryStream {
        base: IDWriteFontFileStream {
            lpVtbl: &STREAM_TABLE,
        },
        references: AtomicU32::new(1),
        bytes,
    });
    // SAFETY: COM passes a valid output, and the reference it gets is released through the table.
    unsafe { *out = Box::into_raw(stream).cast() };
    S_OK
}

/// A COM stream over a font in memory.
#[repr(C)]
struct MemoryStream {
    base: IDWriteFontFileStream,
    references: AtomicU32,
    bytes: FontBytes,
}

static STREAM_TABLE: IDWriteFontFileStreamVtbl = IDWriteFontFileStreamVtbl {
    parent: IUnknownVtbl {
        QueryInterface: stream_query,
        AddRef: stream_add_reference,
        Release: stream_release,
    },
    ReadFileFragment: stream_read,
    ReleaseFileFragment: stream_release_fragment,
    GetFileSize: stream_size,
    GetLastWriteTime: stream_write_time,
};

unsafe extern "system" fn stream_query(
    this: *mut IUnknown,
    id: REFIID,
    out: *mut *mut c_void,
) -> HRESULT {
    // SAFETY: COM passes a valid id and output, and `this` is a live stream.
    unsafe {
        let known = IsEqualGUID(&*id, &IUnknown::uuidof())
            || IsEqualGUID(&*id, &IDWriteFontFileStream::uuidof());
        if !known {
            *out = null_mut();
            return E_NOINTERFACE;
        }
        stream_add_reference(this);
        *out = this.cast();
    }
    S_OK
}

unsafe extern "system" fn stream_add_reference(this: *mut IUnknown) -> ULONG {
    // SAFETY: `this` is a live stream.
    let stream = unsafe { &*this.cast::<MemoryStream>() };
    stream.references.fetch_add(1, Ordering::Relaxed) + 1
}

unsafe extern "system" fn stream_release(this: *mut IUnknown) -> ULONG {
    let stream = this.cast::<MemoryStream>();
    // SAFETY: `this` is a live stream.
    let left = unsafe { (*stream).references.fetch_sub(1, Ordering::AcqRel) } - 1;
    if left == 0 {
        // SAFETY: that was the last reference to what `loader_open` boxed.
        drop(unsafe { Box::from_raw(stream) });
    }
    left
}

unsafe extern "system" fn stream_read(
    this: *mut IDWriteFontFileStream,
    start: *mut *const c_void,
    offset: u64,
    size: u64,
    context: *mut *mut c_void,
) -> HRESULT {
    // SAFETY: `this` is a live stream.
    let contents: &[u8] = unsafe { (*(*this.cast::<MemoryStream>()).bytes).as_ref() };
    let fragment = usize::try_from(offset).ok().zip(usize::try_from(size).ok());
    let fragment =
        fragment.and_then(|(offset, size)| contents.get(offset..offset.checked_add(size)?));
    // SAFETY: COM passes valid outputs; the bytes last as long as the stream, as it requires.
    unsafe {
        *context = null_mut();
        match fragment {
            Some(fragment) => {
                *start = fragment.as_ptr().cast();
                S_OK
            }
            None => {
                *start = null();
                E_FAIL
            }
        }
    }
}

unsafe extern "system" fn stream_release_fragment(_: *mut IDWriteFontFileStream, _: *mut c_void) {}

unsafe extern "system" fn stream_size(this: *mut IDWriteFontFileStream, size: *mut u64) -> HRESULT {
    // SAFETY: `this` is a live stream and COM passes a valid output.
    unsafe {
        let contents: &[u8] = (*(*this.cast::<MemoryStream>()).bytes).as_ref();
        *size = contents.len() as u64;
    }
    S_OK
}

unsafe extern "system" fn stream_write_time(
    _: *mut IDWriteFontFileStream,
    time: *mut u64,
) -> HRESULT {
    // SAFETY: COM passes a valid output. Memory has no write time; zero says so.
    unsafe { *time = 0 };
    S_OK
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    const TUFFY: &[u8] = include_bytes!("../../fonts/Tuffy.ttf");

    fn stream(bytes: &FontBytes) -> *mut MemoryStream {
        Box::into_raw(Box::new(MemoryStream {
            base: IDWriteFontFileStream {
                lpVtbl: &STREAM_TABLE,
            },
            references: AtomicU32::new(1),
            bytes: bytes.clone(),
        }))
    }

    /// `stream_read`'s result, and the bytes it gave.
    unsafe fn read(this: *mut MemoryStream, offset: u64, size: u64) -> (HRESULT, Vec<u8>) {
        let (mut start, mut context) = (null(), null_mut());
        // SAFETY: the caller's stream is live and the outputs are valid.
        unsafe {
            let result = stream_read(this.cast(), &raw mut start, offset, size, &raw mut context);
            let bytes = if result == S_OK {
                from_raw_parts(start.cast::<u8>(), usize::try_from(size).expect("size")).to_vec()
            } else {
                Vec::new()
            };
            (result, bytes)
        }
    }

    #[test]
    fn a_stream_reads_only_what_the_font_holds() {
        let this = stream(&(Arc::new(TUFFY) as FontBytes));
        let past = TUFFY.len() as u64;
        let mut size = 0;
        // SAFETY: a live stream, released once at the end.
        let (inside, beyond, wrapped, sized) = unsafe {
            let results = (
                read(this, 4, 8),
                read(this, past, 1),
                read(this, 1, u64::MAX),
                stream_size(this.cast(), &raw mut size),
            );
            stream_release(this.cast());
            results
        };
        assert_eq!(inside, (S_OK, TUFFY[4..12].to_vec()));
        assert_eq!(beyond.0, E_FAIL);
        assert_eq!(wrapped.0, E_FAIL);
        assert_eq!((sized, size), (S_OK, past));
    }

    #[test]
    fn a_stream_frees_its_font_on_the_last_release() {
        let bytes: FontBytes = Arc::new(TUFFY);
        let this = stream(&bytes).cast::<IUnknown>();
        assert_eq!(Arc::strong_count(&bytes), 2);
        // SAFETY: a live stream, with each reference released once.
        let counts = unsafe {
            (
                stream_add_reference(this),
                stream_release(this),
                stream_release(this),
            )
        };
        assert_eq!(counts, (2, 1, 0));
        assert_eq!(Arc::strong_count(&bytes), 1);
    }

    #[test]
    fn a_font_in_memory_has_no_path_and_gives_its_bytes_back() {
        let face = FontFace::from_bytes(Arc::new(TUFFY), 0).expect("a face");
        assert!(face.glyph_index('H').is_some());
        assert!(face.glyph_index('가').is_none());
        let file = face.file().expect("its file");
        assert!(file.path().is_none());
        assert_eq!(file.bytes().expect("bytes"), TUFFY);
    }

    #[test]
    fn an_installed_font_has_a_path_and_clones_share_it() {
        let collection = FontCollection::system().expect("the system fonts");
        let family = collection.family_by_name("Segoe UI").expect("segoe ui");
        let face = family.matching_face(400, false).expect("a face");
        let copy = face.clone();
        drop(face);
        let path = copy.file().and_then(|file| file.path()).expect("a path");
        assert!(path.exists());
        assert!(collection.family_by_name("__no_such_font__").is_none());
        let names = collection.family_names();
        assert!(names.iter().any(|name| name == "Segoe UI"));
    }

    #[test]
    fn what_is_not_a_font_makes_no_face() {
        assert!(FontFace::from_bytes(Arc::new(b"nope".as_slice()), 0).is_none());
    }
}
