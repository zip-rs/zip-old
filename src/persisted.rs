#[derive(Copy, Clone)]
pub struct Persisted<T, D> {
    pub structure: T,
    pub disk: D,
}
impl<T, D> core::ops::Deref for Persisted<T, D> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.structure
    }
}
impl<T, D> Persisted<T, D> {
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Persisted<U, D> {
        Persisted {
            structure: f(self.structure),
            disk: self.disk,
        }
    }
    pub fn map_disk<U>(self, f: impl FnOnce(D) -> U) -> Persisted<T, U> {
        Persisted {
            structure: self.structure,
            disk: f(self.disk),
        }
    }
    pub fn with_disk<U>(self, disk: U) -> Persisted<T, U> {
        Persisted {
            structure: self.structure,
            disk,
        }
    }
    pub fn try_map_disk<U, E>(
        self,
        f: impl FnOnce(D) -> Result<U, E>,
    ) -> Result<Persisted<T, U>, E> {
        Ok(Persisted {
            structure: self.structure,
            disk: f(self.disk)?,
        })
    }
    pub fn cloned(self) -> Persisted<<T as core::ops::Deref>::Target, D>
    where
        T: core::ops::Deref,
        <T as core::ops::Deref>::Target: Clone,
    {
        Persisted {
            structure: self.structure.clone(),
            disk: self.disk,
        }
    }
    pub fn as_mut(&mut self) -> Persisted<&mut T, &mut D> {
        Persisted {
            structure: &mut self.structure,
            disk: &mut self.disk,
        }
    }
}
