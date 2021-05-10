pub trait FlexiBorrow<'a, T> {
    fn borrow(self) -> &'a T;
}

impl<'a, T> FlexiBorrow<'a, T> for &'a T {
    fn borrow(self) -> &'a T {
        self
    }
}

impl<'a, T> FlexiBorrow<'a, T> for &'a mut T {
    fn borrow(self) -> &'a T {
        self
    }
}

mod test {
    use super::*;
    use std::borrow::BorrowMut;
    use std::marker::PhantomData;

    struct Data(i32);

    impl Data {
        pub fn some_mut_method(&mut self) {
            self.0 += 1;
        }
    }

    struct DataRef<'a, RefType>(RefType, PhantomData<&'a ()>)
    where
        RefType: FlexiBorrow<'a, Data>;

    impl<'a, RefType> DataRef<'a, RefType>
    where
        RefType: FlexiBorrow<'a, Data>,
    {
        pub fn some_method(&self) -> i32 {
            self.0.borrow().0 + 1
        }

        pub fn some_reference(&self) -> &'a i32 {
            &self.0.borrow().0
        }
    }

    impl<'a, RefType> DataRef<'a, RefType>
    where
        RefType: FlexiBorrow<'a, Data> + BorrowMut<Data>,
    {
        pub fn some_mut_method(&mut self) {
            let borrowing_calculation = self.some_method();
            self.0.borrow_mut().0 += 1 + borrowing_calculation;
        }

        pub fn some_mut_reference(&mut self) -> &mut i32 {
            &mut self.0.borrow_mut().0
        }
    }

    struct DumbRef<'a>(&'a mut Data);
    impl<'a> DumbRef<'a> {
        pub fn some_mut_method<'b>(&'b mut self)
        where
            'a: 'b,
        {
            self.0 .0 += 1;
        }
        pub fn some_mut_method_uses(&'a mut self) {
            self.some_mut_method();
            self.some_mut_method();
        }
    }

    fn test() {
        let mut data = Data(3);
        let internal_ref = {
            let data_ref = DataRef(&data, PhantomData);
            data_ref.some_reference()
        };
        println!("{}", internal_ref);
        let mut data_refmut = DataRef(&mut data, PhantomData);
        data_refmut.some_method();
        data_refmut.some_mut_method();
        data_refmut.some_mut_method();
        let internal_refmut = data_refmut.some_mut_reference();
        *internal_refmut += 1;
        println!("{}", internal_refmut);
        data.some_mut_method();
        data.some_mut_method();
    }
}
